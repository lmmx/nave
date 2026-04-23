//! Per-repo pen state, computed on demand.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use tokio::process::Command;

use nave_config::cache::{RepoMeta, read_cache_meta, read_repo_meta};

use crate::storage::{Pen, PenRepo, pen_repo_clone_dir};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkTree {
    Clean,
    Dirty,
    Missing,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Freshness {
    Fresh,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RunState {
    NotRun,
    RunLocal,
    RunPushed,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Divergence {
    UpToDate,
    Ahead,
    Behind,
    Diverged,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoState {
    pub owner: String,
    pub repo: String,
    pub working_tree: WorkTree,
    pub freshness: Freshness,
    pub run_state: RunState,
    pub divergence: Divergence,
    pub ahead: u32,
    pub behind: u32,
}

pub async fn compute_repo_state(
    pen_root: &Path,
    cache_root: &Path,
    pen: &Pen,
    r: &PenRepo,
) -> Result<RepoState> {
    let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
    if !dir.exists() {
        return Ok(RepoState {
            owner: r.owner.clone(),
            repo: r.name.clone(),
            working_tree: WorkTree::Missing,
            freshness: Freshness::Unknown,
            run_state: RunState::NotRun,
            divergence: Divergence::Unknown,
            ahead: 0,
            behind: 0,
        });
    }

    let working_tree = git_working_tree(&dir).await?;
    let freshness = compute_freshness(cache_root, r).unwrap_or(Freshness::Unknown);
    let run_state = compute_run_state(&dir, &pen.branch, &r.default_branch).await?;
    let (divergence, ahead, behind) = compute_divergence(&dir, &pen.branch).await?;

    Ok(RepoState {
        owner: r.owner.clone(),
        repo: r.name.clone(),
        working_tree,
        freshness,
        run_state,
        divergence,
        ahead,
        behind,
    })
}

async fn git_working_tree(dir: &Path) -> Result<WorkTree> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["status", "--porcelain"])
        .output()
        .await?;
    if !out.status.success() {
        return Ok(WorkTree::Missing);
    }
    if out.stdout.iter().all(|b| *b == b'\n' || *b == b' ') {
        Ok(WorkTree::Clean)
    } else {
        Ok(WorkTree::Dirty)
    }
}

fn compute_freshness(cache_root: &Path, r: &PenRepo) -> Result<Freshness> {
    let Some(meta): Option<RepoMeta> = read_repo_meta(cache_root, &r.owner, &r.name)? else {
        // Repo isn't in the fleet cache anymore.
        return Ok(Freshness::Stale);
    };
    let Some(pushed) = meta.pushed_at else {
        return Ok(Freshness::Unknown);
    };
    if pushed > r.synced_at {
        Ok(Freshness::Stale)
    } else {
        Ok(Freshness::Fresh)
    }
}

async fn compute_run_state(dir: &Path, pen_branch: &str, default_branch: &str) -> Result<RunState> {
    // Any commits on pen_branch that aren't on default_branch?
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "rev-list",
            "--count",
            &format!("{default_branch}..{pen_branch}"),
        ])
        .output()
        .await?;
    let local_ahead: u32 = if out.status.success() {
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    } else {
        0
    };
    if local_ahead == 0 {
        return Ok(RunState::NotRun);
    }

    // Does the remote have our pen branch?
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["ls-remote", "--heads", "origin", pen_branch])
        .output()
        .await?;
    if out.status.success() && !out.stdout.is_empty() {
        Ok(RunState::RunPushed)
    } else {
        Ok(RunState::RunLocal)
    }
}

async fn compute_divergence(dir: &Path, pen_branch: &str) -> Result<(Divergence, u32, u32)> {
    // Only meaningful once we've pushed; for local-only branches, use
    // up-to-date as the baseline.
    let remote = format!("origin/{pen_branch}");
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "rev-list",
            "--left-right",
            "--count",
            &format!("{pen_branch}...{remote}"),
        ])
        .output()
        .await?;
    if !out.status.success() {
        return Ok((Divergence::UpToDate, 0, 0));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split_whitespace();
    let ahead: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let behind: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let div = match (ahead, behind) {
        (0, 0) => Divergence::UpToDate,
        (_, 0) => Divergence::Ahead,
        (0, _) => Divergence::Behind,
        _ => Divergence::Diverged,
    };
    Ok((div, ahead, behind))
}

#[allow(dead_code)]
pub fn cache_last_pushed(cache_root: &Path) -> Result<Option<time::OffsetDateTime>> {
    Ok(read_cache_meta(cache_root)?.last_pushed_at)
}
