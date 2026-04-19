use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::Result;
use futures::{StreamExt, TryStreamExt, stream};
use time::OffsetDateTime;
use tracing::{info, warn};

use nave_config::{
    NaveConfig,
    cache::{
        CacheMeta, RepoMeta, TrackedFiles, read_cache_meta, read_tracked, write_cache_meta,
        write_repo_meta, write_tracked,
    },
};
use nave_github::{
    auth::detect_auth,
    client::GithubClient,
    models::{Repo, TreeResponse},
};

/// Concurrency ceiling for tree walks. GitHub's secondary rate limit will bite
/// at ~100 concurrent requests; 8 is conservative and plenty for 40–50 repos.
const TREE_CONCURRENCY: usize = 8;

pub struct DiscoveryReport {
    pub repos_seen: usize,
    pub repos_with_tracked_files: usize,
    pub tracked_file_count: usize,
    pub auth_mode: String,
    pub incremental: bool,
}

pub async fn run_discovery(
    cfg: &NaveConfig,
    cache_root: &Path,
    username: &str,
) -> Result<DiscoveryReport> {
    let auth = detect_auth(cfg.github.use_gh_cli).await;
    let auth_label = auth.label().to_string();
    let client = GithubClient::new(&cfg.github.api_base, auth)?;

    let cache_meta_before = read_cache_meta(cache_root)?;
    let incremental = cache_meta_before.last_pushed_at.is_some()
        && cache_meta_before.username.as_deref() == Some(username);

    let repos: Vec<Repo> = if let (true, Some(ts)) = (incremental, cache_meta_before.last_pushed_at)
    {
        let ts_str = ts.format(&time::format_description::well_known::Rfc3339)?;
        info!(since = %ts_str, "running incremental search");
        client
            .search_user_repos_pushed_since(username, &ts_str)
            .await?
    } else {
        info!("running full repo listing (first run or user changed)");
        client
            .list_user_repos(username, cfg.github.per_page, &cfg.github.repo_type)
            .await?
    };

    info!(count = repos.len(), "repos returned from GitHub");

    // Skip archived and forks by default.
    let repos: Vec<Repo> = repos
        .into_iter()
        .filter(|r| !r.archived)
        .filter(|r| !(cfg.discovery.exclude_forks && r.fork))
        .collect();

    let tracked_set: HashSet<String> = cfg.discovery.tracked_paths.iter().cloned().collect();

    // Walk tree for each repo, in parallel, capped.
    let results: Vec<(Repo, TreeResponse)> = stream::iter(repos)
        .map(|repo| {
            let client = &client;
            async move {
                let (owner, name) = split_full_name(&repo.full_name);
                let tree = client
                    .get_tree_recursive(&owner, &name, &repo.default_branch)
                    .await?;
                Ok::<_, anyhow::Error>((repo, tree))
            }
        })
        .buffer_unordered(TREE_CONCURRENCY)
        .try_collect()
        .await?;

    let mut max_pushed = cache_meta_before.last_pushed_at;
    let mut repos_with_tracked = 0usize;
    let mut tracked_total = 0usize;

    for (repo, tree) in &results {
        let (owner, name) = split_full_name(&repo.full_name);

        // Filter the tree down to just the paths we track.
        let matched: BTreeMap<String, String> = tree
            .tree
            .iter()
            .filter(|e| e.entry_type == "blob")
            .filter(|e| tracked_set.contains(&e.path))
            .map(|e| (e.path.clone(), e.sha.clone()))
            .collect();

        if matched.is_empty() {
            // No tracked files; don't pollute the cache with empty entries.
            continue;
        }

        repos_with_tracked += 1;
        tracked_total += matched.len();

        let repo_meta = RepoMeta {
            owner: owner.clone(),
            name: name.clone(),
            default_branch: repo.default_branch.clone(),
            clone_url: repo.clone_url.clone(),
            tree_sha: Some(tree.sha.clone()),
            pushed_at: repo.pushed_at,
        };
        write_repo_meta(cache_root, &repo_meta)?;

        // Merge with existing so we don't lose state for files that disappeared
        // this run (we want to notice deletions downstream at fetch-time).
        let existing = read_tracked(cache_root, &owner, &name)?;
        let merged = merge_tracked(existing, TrackedFiles { files: matched });
        write_tracked(cache_root, &owner, &name, &merged)?;

        if let Some(pushed) = repo.pushed_at {
            max_pushed = Some(match max_pushed {
                Some(cur) if cur >= pushed => cur,
                _ => pushed,
            });
        }
    }

    if results.is_empty() && incremental {
        info!("no new pushes since last run");
    }

    let new_meta = CacheMeta {
        last_pushed_at: max_pushed,
        last_discovery_at: Some(OffsetDateTime::now_utc()),
        auth_mode: Some(auth_label.clone()),
        username: Some(username.to_string()),
    };
    write_cache_meta(cache_root, &new_meta)?;

    if auth_label == "anonymous" {
        warn!("discovery completed anonymously; results may be rate-limited");
    }

    Ok(DiscoveryReport {
        repos_seen: results.len(),
        repos_with_tracked_files: repos_with_tracked,
        tracked_file_count: tracked_total,
        auth_mode: auth_label,
        incremental,
    })
}

fn split_full_name(full_name: &str) -> (String, String) {
    match full_name.split_once('/') {
        Some((o, n)) => (o.to_string(), n.to_string()),
        None => (String::new(), full_name.to_string()),
    }
}

/// Union of old and new: preserves any files we knew about previously.
/// Fetch-time logic will compare against reality and reconcile deletions.
fn merge_tracked(old: TrackedFiles, new: TrackedFiles) -> TrackedFiles {
    let mut files = old.files;
    for (k, v) in new.files {
        files.insert(k, v);
    }
    TrackedFiles { files }
}
