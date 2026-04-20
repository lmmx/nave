//! Sparse-checkout puller.
//!
//! For each repo directory under `<cache_root>/repos/<owner>/<repo>/`, ensure
//! `checkout/` contains exactly the files listed in `tracked.toml`.

mod git;
mod plan;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use futures::{StreamExt, stream};
use tracing::{debug, info, warn};

use nave_config::cache::{RepoMeta, TrackedFiles, read_repo_meta, read_tracked, write_tracked};

use crate::git::GitRunner;
use crate::plan::{PullAction, PullPlan};

pub const FETCH_CONCURRENCY: usize = 6;

#[derive(Debug, Default)]
pub struct PullReport {
    pub cloned: usize,
    pub updated: usize,
    pub skipped: usize,
    pub recloned: usize,
    pub failed: usize,
    pub sha_mismatches: usize,
}

pub async fn run_pull(cache_root: &Path) -> Result<PullReport> {
    let repos = scan_cached_repos(cache_root)?;
    info!(count = repos.len(), "planning pull");

    let results: Vec<Result<PullRepoResult>> = stream::iter(repos)
        .map(|repo_dir| async move { pull_one(&repo_dir).await })
        .buffer_unordered(FETCH_CONCURRENCY)
        .collect()
        .await;

    let mut report = PullReport::default();
    for r in results {
        match r {
            Ok(outcome) => {
                report.sha_mismatches += outcome.sha_mismatches;
                match outcome.action {
                    PullAction::FreshClone => report.cloned += 1,
                    PullAction::Update => report.updated += 1,
                    PullAction::Reclone => report.recloned += 1,
                    PullAction::Skip => report.skipped += 1,
                }
            }
            Err(e) => {
                report.failed += 1;
                warn!("pull failed: {e:#}");
            }
        }
    }
    Ok(report)
}

struct PullRepoResult {
    action: PullAction,
    sha_mismatches: usize,
}

async fn pull_one(repo_dir: &Path) -> Result<PullRepoResult> {
    let meta = read_repo_meta_required(repo_dir)?;
    // TODO: make this less ugly - doing it this way rather than threading cache_root through
    // because it keeps pull_one self-contained and the relationship is an invariant of the cache
    // layout not a runtime concern
    let tracked = read_tracked(
        repo_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap(),
        &meta.owner,
        &meta.name,
    )?;

    if tracked.files.is_empty() {
        debug!(owner = %meta.owner, name = %meta.name, "no tracked files; skipping");
        return Ok(PullRepoResult {
            action: PullAction::Skip,
            sha_mismatches: 0,
        });
    }

    let checkout_dir = repo_dir.join("checkout");
    let git = GitRunner::new();
    let plan = PullPlan::decide(&checkout_dir, &tracked);

    let action = match plan {
        PullAction::Skip => {
            debug!(owner = %meta.owner, name = %meta.name, "checkout already current");
            PullAction::Skip
        }
        PullAction::FreshClone => {
            fresh_clone(&git, &meta, &tracked, &checkout_dir).await?;
            PullAction::FreshClone
        }
        PullAction::Update => match update_checkout(&git, &meta, &tracked, &checkout_dir).await {
            Ok(()) => PullAction::Update,
            Err(e) => {
                warn!(
                    owner = %meta.owner, name = %meta.name,
                    "update failed ({e:#}); falling back to reclone",
                );
                reclone(&git, &meta, &tracked, &checkout_dir).await?;
                PullAction::Reclone
            }
        },
        PullAction::Reclone => {
            reclone(&git, &meta, &tracked, &checkout_dir).await?;
            PullAction::Reclone
        }
    };

    // Regardless of which path we took, verify SHAs and reconcile the cache.
    let mismatches = verify_and_reconcile(&git, repo_dir, &meta, &tracked, &checkout_dir).await?;

    Ok(PullRepoResult {
        action,
        sha_mismatches: mismatches,
    })
}

async fn fresh_clone(
    git: &GitRunner,
    meta: &RepoMeta,
    tracked: &TrackedFiles,
    checkout_dir: &Path,
) -> Result<()> {
    if checkout_dir.exists() {
        std::fs::remove_dir_all(checkout_dir)?;
    }
    if let Some(parent) = checkout_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    git.clone_sparse(&meta.clone_url, checkout_dir).await?;
    let paths: Vec<&str> = tracked.files.keys().map(String::as_str).collect();
    git.sparse_checkout_set(checkout_dir, &paths).await?;
    Ok(())
}

async fn update_checkout(
    git: &GitRunner,
    meta: &RepoMeta,
    tracked: &TrackedFiles,
    checkout_dir: &Path,
) -> Result<()> {
    git.fetch(checkout_dir).await?;
    git.reset_hard(checkout_dir, &meta.default_branch).await?;
    let paths: Vec<&str> = tracked.files.keys().map(String::as_str).collect();
    git.sparse_checkout_set(checkout_dir, &paths).await?;
    Ok(())
}

async fn reclone(
    git: &GitRunner,
    meta: &RepoMeta,
    tracked: &TrackedFiles,
    checkout_dir: &Path,
) -> Result<()> {
    fresh_clone(git, meta, tracked, checkout_dir).await
}

async fn verify_and_reconcile(
    git: &GitRunner,
    repo_dir: &Path,
    meta: &RepoMeta,
    tracked: &TrackedFiles,
    checkout_dir: &Path,
) -> Result<usize> {
    let mut mismatches = 0usize;
    let mut updates: Vec<(String, String)> = Vec::new();

    for (path, cached_sha) in &tracked.files {
        let on_disk = checkout_dir.join(path);
        if !on_disk.exists() {
            // File tracked in cache but not materialized; sparse spec may differ
            // from what upstream actually has. Record as mismatch; scan will
            // reconcile next run.
            warn!(
                owner = %meta.owner, name = %meta.name, path = %path,
                "tracked file missing from checkout",
            );
            mismatches += 1;
            continue;
        }
        let actual = git.hash_object(&on_disk).await?;
        if actual != *cached_sha {
            mismatches += 1;
            updates.push((path.clone(), actual));
        }
    }

    if !updates.is_empty() {
        warn!(
            owner = %meta.owner, name = %meta.name, count = updates.len(),
            "tracked SHAs drifted; updating cache to match checkout",
        );
        let cache_root = repo_dir
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .context("repo_dir has no cache root ancestor")?;
        let mut merged = tracked.clone();
        for (path, sha) in updates {
            merged.files.insert(path, sha);
        }
        write_tracked(cache_root, &meta.owner, &meta.name, &merged)?;
    }

    Ok(mismatches)
}

fn read_repo_meta_required(repo_dir: &Path) -> Result<RepoMeta> {
    // repo_dir is <cache_root>/repos/<owner>/<repo>
    let owner = repo_dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .context("could not infer owner from path")?;
    let repo = repo_dir
        .file_name()
        .and_then(|s| s.to_str())
        .context("could not infer repo name from path")?;
    let cache_root = repo_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("repo_dir has no cache root ancestor")?;
    read_repo_meta(cache_root, owner, repo)?
        .with_context(|| format!("meta.toml missing for {owner}/{repo}"))
}

/// Walk `<cache_root>/repos/<owner>/<repo>/` and return each repo directory.
fn scan_cached_repos(cache_root: &Path) -> Result<Vec<PathBuf>> {
    let repos_root = cache_root.join("repos");
    if !repos_root.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for owner_entry in std::fs::read_dir(&repos_root)? {
        let owner_entry = owner_entry?;
        if !owner_entry.file_type()?.is_dir() {
            continue;
        }
        for repo_entry in std::fs::read_dir(owner_entry.path())? {
            let repo_entry = repo_entry?;
            if !repo_entry.file_type()?.is_dir() {
                continue;
            }
            out.push(repo_entry.path());
        }
    }
    Ok(out)
}
