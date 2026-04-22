//! Mutation operations on a pen: sync, clean, revert, reinit, exec.

use std::path::Path;

use anyhow::{Context, Result, bail};
use time::OffsetDateTime;
use tokio::process::Command;
use tracing::info;

use crate::state::compute_repo_state;
use crate::storage::{Pen, pen_repo_clone_dir, write_pen};

/// Re-read fleet metadata, update each repo's `synced_at`, and optionally
/// fetch from origin. Returns `(updated_count, stale_before)`.
pub async fn sync_pen(
    pen_root: &Path,
    cache_root: &Path,
    pen: &mut Pen,
    dry_run: bool,
) -> Result<SyncReport> {
    let mut report = SyncReport::default();
    let pen_snapshot = pen.clone();

    for r in &mut pen.repos {
        let mut single_repo_pen = pen_snapshot.clone();
        single_repo_pen.repos = vec![r.clone()];

        let before = compute_repo_state(pen_root, cache_root, &single_repo_pen, r).await?;
        if before.freshness == crate::state::Freshness::Stale {
            report.stale_repos.push(format!("{}/{}", r.owner, r.name));
        }
        if dry_run {
            continue;
        }
        // git fetch inside the pen repo so downstream ops see fresh remote refs
        let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
        if dir.exists() {
            let _ = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["fetch", "--depth=1", "origin"])
                .status()
                .await;
        }
        // Stamp the new synced_at
        r.synced_at = OffsetDateTime::now_utc();
        report.freshened += 1;
    }
    if !dry_run {
        write_pen(pen_root, pen)?;
    }
    Ok(report)
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub freshened: usize,
    pub stale_repos: Vec<String>,
}

/// Discard uncommitted changes in every pen repo.
pub async fn clean_pen(pen_root: &Path, pen: &Pen) -> Result<()> {
    for r in &pen.repos {
        let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
        if !dir.exists() {
            continue;
        }
        run_git(&dir, &["reset", "--hard", "HEAD"], "reset").await?;
        run_git(&dir, &["clean", "-fd"], "clean").await?;
        info!(repo = %format!("{}/{}", r.owner, r.name), "cleaned");
    }
    Ok(())
}

/// Drop all local commits made on the pen branch, returning each repo
/// to the synced baseline (i.e. the pen branch's initial creation point).
pub async fn revert_pen(pen_root: &Path, pen: &Pen, allow_dirty: bool) -> Result<()> {
    // Pre-flight: no dirty unless --allow-dirty.
    if !allow_dirty {
        for r in &pen.repos {
            let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
            if !dir.exists() {
                continue;
            }
            if !working_tree_clean(&dir).await? {
                bail!(
                    "repo {}/{} has uncommitted changes; pass --allow-dirty to discard",
                    r.owner,
                    r.name
                );
            }
        }
    }
    for r in &pen.repos {
        let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
        if !dir.exists() {
            continue;
        }
        if allow_dirty {
            run_git(&dir, &["reset", "--hard", "HEAD"], "reset").await?;
            run_git(&dir, &["clean", "-fd"], "clean").await?;
        }
        // Reset pen branch to default branch's tip (the synced baseline).
        run_git(
            &dir,
            &["reset", "--hard", &r.default_branch],
            "reset to default",
        )
        .await?;
        info!(repo = %format!("{}/{}", r.owner, r.name), "reverted");
    }
    Ok(())
}

/// Rebuild the pen branch from origin's default branch. Equivalent to
/// reclone-and-rebranch in place.
pub async fn reinit_pen(pen_root: &Path, pen: &Pen, allow_dirty: bool) -> Result<()> {
    if !allow_dirty {
        for r in &pen.repos {
            let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
            if !dir.exists() {
                continue;
            }
            if !working_tree_clean(&dir).await? {
                bail!(
                    "repo {}/{} has uncommitted changes; pass --allow-dirty to discard",
                    r.owner,
                    r.name
                );
            }
        }
    }
    for r in &pen.repos {
        let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
        if !dir.exists() {
            continue;
        }
        if allow_dirty {
            run_git(&dir, &["reset", "--hard", "HEAD"], "reset").await?;
            run_git(&dir, &["clean", "-fd"], "clean").await?;
        }
        // Fetch the default branch's tip, then reset pen branch to it.
        run_git(
            &dir,
            &["fetch", "--depth=1", "origin", &r.default_branch],
            "fetch",
        )
        .await?;
        run_git(
            &dir,
            &["reset", "--hard", &format!("origin/{}", r.default_branch)],
            "reset to origin default",
        )
        .await?;
        // Ensure we're on the pen branch.
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["checkout", "-B", &pen.branch])
            .status()
            .await;
        info!(repo = %format!("{}/{}", r.owner, r.name), "reinitialised");
    }
    Ok(())
}

/// Run `cmd` (as a shell command) in each pen repo. If `commit` is
/// set, commit any resulting changes with a generated message (or
/// user-supplied message). If `push` is set, push after committing.
pub async fn exec_pen(
    pen_root: &Path,
    pen: &Pen,
    cmd: &[String],
    only: Option<&str>,
    commit: bool,
    push: bool,
    commit_message: Option<&str>,
) -> Result<()> {
    if cmd.is_empty() {
        bail!("exec requires a command after `--`");
    }
    for r in &pen.repos {
        if let Some(only_name) = only {
            let label = format!("{}/{}", r.owner, r.name);
            if label != only_name && r.name != only_name {
                continue;
            }
        }
        let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
        if !dir.exists() {
            continue;
        }

        info!(repo = %format!("{}/{}", r.owner, r.name), cmd = ?cmd, "exec");
        let status = Command::new(&cmd[0])
            .args(&cmd[1..])
            .current_dir(&dir)
            .status()
            .await?;
        if !status.success() {
            bail!("exec failed in {}/{}", r.owner, r.name);
        }

        if commit {
            run_git(&dir, &["add", "-A"], "add").await?;
            // Skip commit if nothing changed.
            let diff = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["diff", "--cached", "--quiet"])
                .status()
                .await?;
            if !diff.success() {
                let msg = commit_message.unwrap_or("nave pen exec");
                run_git(&dir, &["commit", "-m", msg], "commit").await?;
            }
        }

        if push {
            run_git(
                &dir,
                &["push", "--set-upstream", "origin", &pen.branch],
                "push",
            )
            .await?;
        }
    }
    Ok(())
}

async fn run_git(dir: &Path, args: &[&str], label: &str) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .await
        .with_context(|| format!("spawning git {label}"))?;
    if !status.success() {
        bail!("git {label} failed in {}", dir.display());
    }
    Ok(())
}

async fn working_tree_clean(dir: &Path) -> Result<bool> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["status", "--porcelain"])
        .output()
        .await?;
    Ok(out.status.success() && out.stdout.iter().all(|b| *b == b'\n' || *b == b' '))
}

pub async fn remove_pen_safe(pen_root: &Path, pen: &Pen, allow_dirty: bool) -> Result<()> {
    if !allow_dirty {
        for r in &pen.repos {
            let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
            if !dir.exists() {
                continue;
            }
            if !working_tree_clean(&dir).await? {
                bail!(
                    "repo {}/{} has uncommitted changes; pass --allow-dirty to delete anyway",
                    r.owner,
                    r.name
                );
            }
        }
    }
    crate::storage::remove_pen(pen_root, &pen.name)?;
    Ok(())
}
