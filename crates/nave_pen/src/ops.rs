//! Mutation operations on a pen: sync, clean, revert, reinit, exec.

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use time::OffsetDateTime;
use tokio::process::Command;
use tracing::info;

use crate::rewrite_state::{
    RunLogEntry, RunOutcome, append_run_log, new_run_id, ops_toml_path, read_ops_state,
};
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

/// Discard uncommitted changes in every pen repo. Also clears any
/// per-repo rewrite state for the cleaned repos (since the working
/// tree no longer reflects whatever was applied).
pub async fn clean_pen(pen_root: &Path, pen: &mut Pen) -> Result<()> {
    let mut cleaned = 0usize;
    let mut skipped = 0usize;
    let mut affected: Vec<(String, String)> = Vec::new();
    for r in &pen.repos {
        let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
        if !dir.exists() {
            continue;
        }
        if working_tree_clean(&dir).await? {
            skipped += 1;
            continue;
        }
        run_git_quiet(&dir, &["reset", "--hard", "HEAD"], "reset").await?;
        run_git_quiet(&dir, &["clean", "-fd"], "clean").await?;
        info!(repo = %format!("{}/{}", r.owner, r.name), "cleaned");
        cleaned += 1;
        affected.push((r.owner.clone(), r.name.clone()));
    }
    info!(cleaned, skipped, "clean complete");

    let affected_refs: Vec<(&str, &str)> = affected
        .iter()
        .map(|(o, n)| (o.as_str(), n.as_str()))
        .collect();
    clear_rewrite_state_for(pen_root, pen, &affected_refs)?;
    Ok(())
}

/// Drop all local commits made on the pen branch, returning each repo
/// to the synced baseline (i.e. the pen branch's initial creation point).
/// Also clears per-repo rewrite state for every repo touched, since the
/// working tree no longer reflects whatever was applied.
pub async fn revert_pen(pen_root: &Path, pen: &mut Pen, allow_dirty: bool) -> Result<()> {
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
    let mut affected: Vec<(String, String)> = Vec::new();
    for r in &pen.repos {
        let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
        if !dir.exists() {
            continue;
        }
        if allow_dirty {
            run_git_quiet(&dir, &["reset", "--hard", "HEAD"], "reset").await?;
            run_git_quiet(&dir, &["clean", "-fd"], "clean").await?;
        }
        // Reset pen branch to default branch's tip (the synced baseline).
        run_git_quiet(
            &dir,
            &["reset", "--hard", &r.default_branch],
            "reset to default",
        )
        .await?;
        info!(repo = %format!("{}/{}", r.owner, r.name), "reverted");
        affected.push((r.owner.clone(), r.name.clone()));
    }
    let affected_refs: Vec<(&str, &str)> = affected
        .iter()
        .map(|(o, n)| (o.as_str(), n.as_str()))
        .collect();
    clear_rewrite_state_for(pen_root, pen, &affected_refs)?;
    Ok(())
}

/// Rebuild the pen branch from origin's default branch. Equivalent to
/// reclone-and-rebranch in place. Also clears per-repo rewrite state
/// for every repo touched, since the working tree no longer reflects
/// whatever was applied.
pub async fn reinit_pen(pen_root: &Path, pen: &mut Pen, allow_dirty: bool) -> Result<()> {
    use futures::{StreamExt, stream};

    // Pre-flight: check cleanliness sequentially (cheap, and fails fast).
    if !allow_dirty {
        for r in &pen.repos {
            let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
            if dir.exists() && !working_tree_clean(&dir).await? {
                bail!(
                    "repo {}/{} has uncommitted changes; pass --allow-dirty to discard",
                    r.owner,
                    r.name
                );
            }
        }
    }

    // Collect affected repos before the async block so we can hand them
    // off to `clear_rewrite_state_for` after. Every repo with an existing
    // clone dir is "affected" because reinit forces it back to origin's
    // default branch tip, dropping anything the rewrite did.
    let mut affected: Vec<(String, String)> = Vec::new();
    for r in &pen.repos {
        let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
        if dir.exists() {
            affected.push((r.owner.clone(), r.name.clone()));
        }
    }

    let results: Vec<Result<()>> = stream::iter(pen.repos.iter())
        .map(|r| {
            let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
            let pen_branch = pen.branch.clone();
            let default_branch = r.default_branch.clone();
            let label = format!("{}/{}", r.owner, r.name);
            async move {
                if !dir.exists() {
                    return Ok(());
                }
                if allow_dirty {
                    run_git_quiet(&dir, &["reset", "--hard", "HEAD"], "reset").await?;
                    run_git_quiet(&dir, &["clean", "-fd"], "clean").await?;
                }
                run_git_quiet(
                    &dir,
                    &["fetch", "--depth=1", "origin", &default_branch],
                    "fetch",
                )
                .await?;
                run_git_quiet(
                    &dir,
                    &["reset", "--hard", &format!("origin/{default_branch}")],
                    "reset to origin default",
                )
                .await?;
                run_git_quiet(&dir, &["checkout", "-B", &pen_branch], "checkout branch").await?;
                info!(repo = %label, "reinitialised");
                Ok(())
            }
        })
        .buffer_unordered(6)
        .collect()
        .await;

    for r in results {
        r?;
    }

    let affected_refs: Vec<(&str, &str)> = affected
        .iter()
        .map(|(o, n)| (o.as_str(), n.as_str()))
        .collect();
    clear_rewrite_state_for(pen_root, pen, &affected_refs)?;
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
            run_git_quiet(&dir, &["add", "-A"], "add").await?;
            // Skip commit if nothing changed.
            let diff = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["diff", "--cached", "--quiet"])
                .status()
                .await?;
            if !diff.success() {
                let msg = commit_message.unwrap_or("nave pen exec");
                run_git_quiet(&dir, &["commit", "-m", msg], "commit").await?;
            }
        }

        if push {
            run_git_quiet(
                &dir,
                &["push", "--set-upstream", "origin", &pen.branch],
                "push",
            )
            .await?;
        }
    }
    Ok(())
}

enum GitOutput {
    Status,
    #[allow(dead_code)]
    Output,
}

async fn run_git_impl(dir: &Path, args: &[&str], label: &str, mode: GitOutput) -> Result<()> {
    match mode {
        GitOutput::Status => {
            let status = Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await
                .with_context(|| format!("spawning git {label}"))?;

            if !status.success() {
                bail!("git {label} failed in {}", dir.display());
            }
        }

        GitOutput::Output => {
            let out = Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .with_context(|| format!("spawning git {label}"))?;

            if !out.status.success() {
                bail!(
                    "git {label} failed in {}: {}",
                    dir.display(),
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }
    }

    Ok(())
}

async fn run_git_quiet(dir: &Path, args: &[&str], label: &str) -> Result<()> {
    run_git_impl(dir, args, label, GitOutput::Status).await
}

#[allow(dead_code)]
async fn run_git_with_output(dir: &Path, args: &[&str], label: &str) -> Result<()> {
    run_git_impl(dir, args, label, GitOutput::Output).await
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

/// Clear per-repo rewrite state for the given repos, then recompute
/// the pen-level op statuses and persist them.
///
/// Called from `clean_pen`, `revert_pen`, and `reinit_pen` to keep the
/// rewrite state consistent with what's actually present in the repo
/// working trees and commits. Without this, a rewrite that gets reverted
/// would still show as `applied` in `pen.toml`, and the next
/// `pen rewrite` would skip it.
fn clear_rewrite_state_for(
    pen_root: &Path,
    pen: &mut Pen,
    affected: &[(&str, &str)],
) -> Result<()> {
    if pen.ops.is_empty() {
        return Ok(());
    }

    let run_id = new_run_id();
    let now = OffsetDateTime::now_utc();

    for (owner, name) in affected {
        let ops_toml = ops_toml_path(pen_root, &pen.name, owner, name);

        // Note which ops were applied here, for the run log.
        let prior = read_ops_state(pen_root, &pen.name, owner, name)?;
        if prior.ops.is_empty() && prior.failed.is_empty() {
            continue;
        }

        if ops_toml.exists() {
            std::fs::remove_file(&ops_toml)?;
        }

        for op_id in prior.ops.keys().chain(prior.failed.keys()) {
            append_run_log(
                pen_root,
                &pen.name,
                owner,
                name,
                RunLogEntry {
                    run_id: run_id.clone(),
                    op_id: op_id.clone(),
                    ts: now,
                    outcome: RunOutcome::Skipped,
                    files: vec![],
                    addresses: vec![],
                    reason: Some("state cleared by pen clean/revert/reinit".into()),
                    logs_dir: None,
                },
            )?;
        }
    }

    // Recompute pen-level statuses and persist.
    let statuses =
        crate::rewrite::aggregate_op_statuses(pen_root, &pen.name, &pen.ops, &pen.repos)?;
    for op in &mut pen.ops {
        if let Some(s) = statuses.get(&op.id) {
            op.status = *s;
        }
    }
    write_pen(pen_root, pen)?;
    Ok(())
}
