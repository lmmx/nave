//! Materialise a pen: run the filter, clone matching repos, write pen.toml.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use time::OffsetDateTime;
use tokio::process::Command;
use tracing::{debug, info};

use nave_config::cache::read_repo_meta;
use nave_config::{NaveConfig, Term, cache_root};
use nave_search::{SearchOptions, run_search};

use crate::storage::{
    Pen, PenFilter, PenRepo, pen_dir, pen_repo_clone_dir, resolve_pen_root, write_pen,
};

pub struct CreateOptions {
    pub name: Option<String>,
    pub terms: Vec<String>,
    pub match_preds: Vec<String>,
    pub ignore_case: bool,
}

pub async fn create_pen(cfg: &NaveConfig, opts: CreateOptions) -> Result<Pen> {
    if opts.terms.is_empty() {
        bail!("pen create requires at least one filter term");
    }
    let root = resolve_pen_root(&cfg.pen)?;
    std::fs::create_dir_all(&root)?;

    let cache = cache_root()?;
    let parsed_terms: Vec<Term> = opts
        .terms
        .iter()
        .map(|s| Term::parse(s).with_context(|| format!("parsing term {s:?}")))
        .collect::<Result<_>>()?;

    let parsed_preds: Vec<nave_config::MatchPredicate> = opts
        .match_preds
        .iter()
        .map(|s| {
            nave_config::MatchPredicate::parse(s)
                .with_context(|| format!("parsing --match predicate {s:?}"))
        })
        .collect::<Result<_>>()?;

    let search_opts = SearchOptions {
        terms: parsed_terms,
        match_preds: parsed_preds,
        ignore_case: opts.ignore_case,
        enrich_holes: false,
    };
    let report = run_search(&cache, cfg, &search_opts)?;

    if report.repos.is_empty() {
        bail!("filter matched no repos");
    }

    let name = opts
        .name
        .clone()
        .unwrap_or_else(|| derive_name(&opts.terms));
    let name = ensure_name_unique(&root, &name)?;
    let branch = name.clone();

    info!(pen = %name, count = report.repos.len(), "creating pen");

    let mut pen_repos = Vec::with_capacity(report.repos.len());
    for repo_match in &report.repos {
        let owner = &repo_match.owner;
        let name_r = &repo_match.repo;
        let meta = read_repo_meta(&cache, owner, name_r)?
            .ok_or_else(|| anyhow!("no cached meta for {owner}/{name_r}; run `nave scan` first"))?;

        let dest = pen_repo_clone_dir(&root, &name, owner, name_r);
        if dest.exists() {
            debug!(path = %dest.display(), "clone dir exists, skipping");
        } else {
            clone_and_branch(&meta.clone_url, &meta.default_branch, &branch, &dest).await?;
        }

        pen_repos.push(PenRepo {
            owner: owner.clone(),
            name: name_r.clone(),
            default_branch: meta.default_branch.clone(),
            clone_url: meta.clone_url.clone(),
            synced_at: OffsetDateTime::now_utc(),
        });
    }

    let pen = Pen {
        name: name.clone(),
        created_at: OffsetDateTime::now_utc(),
        branch,
        filter: PenFilter { terms: opts.terms },
        repos: pen_repos,
    };
    write_pen(&root, &pen)?;
    Ok(pen)
}

fn derive_name(terms: &[String]) -> String {
    let raw = terms.first().cloned().unwrap_or_else(|| "pen".to_string());
    // Strip any scope prefix (e.g. "workflow:maturin" -> "maturin").
    // Spurious lint https://github.com/rust-lang/rust-clippy/issues/16901
    #[allow(clippy::map_unwrap_or)]
    let after_scope = raw.split_once(':').map(|(_, v)| v).unwrap_or(&raw);
    let slug = slugify(after_scope);
    let truncated: String = slug.chars().take(20).collect();
    format!("nave/{truncated}")
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_hyphen = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_hyphen = false;
        } else if !prev_hyphen && !out.is_empty() {
            out.push('-');
            prev_hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "pen".to_string()
    } else {
        out
    }
}

fn ensure_name_unique(root: &Path, name: &str) -> Result<String> {
    if !pen_dir(root, name).exists() {
        return Ok(name.to_string());
    }
    for n in 2..1000 {
        let candidate = format!("{name}-{n}");
        if !pen_dir(root, &candidate).exists() {
            return Ok(candidate);
        }
    }
    bail!("could not find a unique name for pen {name}")
}

async fn clone_and_branch(
    clone_url: &str,
    default_branch: &str,
    pen_branch: &str,
    dest: &Path,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Full (non-sparse) but depth=1 shallow clone.
    let out = Command::new("git")
        .args(["clone", "--depth=1"])
        .arg(clone_url)
        .arg(dest)
        .output()
        .await?;
    if !out.status.success() {
        bail!(
            "git clone failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(dest)
        .args(["checkout", "-b", pen_branch, default_branch])
        .output()
        .await?;
    if !out.status.success() {
        // Branch already exists — just check it out.
        let out2 = Command::new("git")
            .arg("-C")
            .arg(dest)
            .args(["checkout", pen_branch])
            .output()
            .await?;
        if !out2.status.success() {
            bail!(
                "git checkout {pen_branch} failed: {}",
                String::from_utf8_lossy(&out2.stderr).trim()
            );
        }
    }
    Ok(())
}
