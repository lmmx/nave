//! Orchestrate declarative rewrites across a pen's repos.
//!
//! Responsibilities:
//!   - Pre-flight: parse all selectors, gate on dirty trees.
//!   - Per-repo: stage all ops in memory; commit-or-rollback per repo.
//!   - State: write per-repo `ops.toml` (in `crate::rewrite_state`),
//!     append run log, write per-failure log artefacts.
//!   - Aggregation: compute per-op pen-level status, persist to `pen.toml`.
//!
//! Sequential v1; layout designed for `tokio::spawn` per repo later.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use time::OffsetDateTime;
use tokio::process::Command;
use tracing::{info, warn};

use nave_config::{NaveConfig, ScanConfig};
use nave_parse::{Document, parse_bytes, render, to_json};
use nave_rewrite::{OpOutcome, OpStatus, RewriteOp, apply_at, plan_rewrite};
use nave_schemas::{SchemaRegistry, schema_for_path};

use crate::rewrite_state::{
    AppliedRecord, FailedRecord, RunLogEntry, RunOutcome, append_run_log, new_run_id,
    read_ops_state, write_op_logs, write_ops_state, OpLogArtefacts,
};
use crate::storage::{Pen, PenRepo, pen_repo_clone_dir, write_pen};
use crate::walk::{TrackedFile, tracked_files_in_repo};

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default, Clone)]
pub struct RewriteOptions {
    /// Restrict to a single repo (matched as bare name or owner/name).
    pub only: Option<String>,
    /// Restrict to specific op ids. Empty = all ops not yet applied
    /// (or all ops, if `force` is set).
    pub op_ids: Vec<String>,
    /// Plan and validate without writing or recording state.
    pub dry_run: bool,
    /// Compute and emit unified diffs (implies `dry_run`).
    pub diff: bool,
    /// Bypass the dirty-tree gate.
    pub allow_dirty: bool,
    /// Skip post-mutation schema validation.
    pub no_validate: bool,
    /// Re-run ops even if already applied for a repo.
    pub force: bool,
    /// Disable per-repo atomic rollback. Failures leave partial work
    /// in the working tree.
    pub no_rollback: bool,
}

#[derive(Debug, Serialize)]
pub struct RewritePenReport {
    pub pen: String,
    pub run_id: String,
    pub repos: Vec<RewriteRepoOutcome>,
    /// Op id → pen-level status after this run.
    pub op_statuses: BTreeMap<String, OpStatus>,
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
pub struct RewriteRepoOutcome {
    pub owner: String,
    pub repo: String,
    pub ops: Vec<RewriteOpOutcome>,
    /// Working-tree-level outcome: did this repo's transaction commit?
    pub committed: bool,
    /// If the repo was rolled back, the op id that caused it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_trigger: Option<String>,
    /// Absolute path to this run's log directory for this repo, if any
    /// log artefacts were written. Built from real path helpers so the
    /// printer doesn't need to know layout conventions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs_dir: Option<PathBuf>,
    /// Unified diffs per file (only populated when `--diff`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diffs: Vec<FileDiff>,
}

#[derive(Debug, Serialize)]
pub struct RewriteOpOutcome {
    pub op_id: String,
    pub outcome: OpOutcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub diff: String,
}

#[allow(clippy::too_many_lines)]
pub async fn rewrite_pen(
    pen_root: &Path,
    cfg: &NaveConfig,
    pen: &mut Pen,
    options: RewriteOptions,
) -> Result<RewritePenReport> {
    if pen.ops.is_empty() {
        bail!("pen {} has no ops defined; nothing to rewrite", pen.name);
    }

    if options.no_rollback && !options.dry_run && !options.diff {
        eprintln!(
            "warning: --no-rollback set; failed rewrites will leave partial changes in working tree"
        );
    }

    // -------- pre-flight: parse selectors --------
    // Parse against an empty value to surface predicate-syntax errors
    // early; resolution against real trees happens per file later.
    for op in &pen.ops {
        plan_rewrite(op, &serde_json::Value::Null)
            .with_context(|| format!("op {:?} has malformed selector", op.id))?;
    }

    // -------- pre-flight: choose repo set --------
    let selected: Vec<&PenRepo> = pen
        .repos
        .iter()
        .filter(|r| repo_matches_only(r, options.only.as_deref()))
        .collect();
    if selected.is_empty() {
        bail!("no repos selected (check --only)");
    }

    // -------- pre-flight: dirty tree gate (single pass, surface all) --------
    if !options.allow_dirty && !options.dry_run && !options.diff {
        let mut dirty = Vec::new();
        for r in &selected {
            let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
            if !dir.exists() {
                continue;
            }
            if !working_tree_clean(&dir).await? {
                dirty.push(format!("{}/{}", r.owner, r.name));
            }
        }
        if !dirty.is_empty() {
            bail!(
                "refusing to rewrite: {} repo(s) have uncommitted changes; resolve via `pen clean`, commit, or pass --allow-dirty:\n  {}",
                dirty.len(),
                dirty.join("\n  ")
            );
        }
    }

    // -------- pre-flight: choose ops in scope --------
    let op_filter: Option<BTreeSet<&str>> = if options.op_ids.is_empty() {
        None
    } else {
        Some(options.op_ids.iter().map(String::as_str).collect())
    };
    let in_scope_ops: Vec<&RewriteOp> = pen
        .ops
        .iter()
        .filter(|o| op_filter.as_ref().is_none_or(|f| f.contains(o.id.as_str())))
        .collect();
    if in_scope_ops.is_empty() {
        bail!("no ops selected (check --op)");
    }

    // -------- schema registry (lazy compile) --------
    let registry = if options.no_validate {
        None
    } else {
        let cache = nave_config::cache_root()?;
        let reg = SchemaRegistry::new(&cache, cfg.schemas.clone())?;
        Some(reg)
    };

    let run_id = new_run_id();
    let mut report = RewritePenReport {
        pen: pen.name.clone(),
        run_id: run_id.clone(),
        repos: Vec::new(),
        op_statuses: BTreeMap::new(),
        dry_run: options.dry_run || options.diff,
    };

    for r in &selected {
        let outcome = rewrite_one_repo(
            pen_root,
            &pen.name,
            &cfg.scan,
            r,
            &in_scope_ops,
            &options,
            &run_id,
            registry.as_ref(),
        )?;
        report.repos.push(outcome);
    }

    // -------- aggregation: per-op pen-level status --------
    if report.dry_run {
        for op in &pen.ops {
            report.op_statuses.insert(op.id.clone(), op.status);
        }
    } else {
        let statuses = aggregate_op_statuses(pen_root, &pen.name, &pen.ops, &pen.repos)?;
        report.op_statuses.clone_from(&statuses);
        for op in &mut pen.ops {
            if let Some(s) = statuses.get(&op.id) {
                op.status = *s;
            }
        }
        write_pen(pen_root, pen)?;
    }

    Ok(report)
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn rewrite_one_repo(
    pen_root: &Path,
    pen_name: &str,
    scan: &ScanConfig,
    r: &PenRepo,
    ops: &[&RewriteOp],
    options: &RewriteOptions,
    run_id: &str,
    registry: Option<&SchemaRegistry>,
) -> Result<RewriteRepoOutcome> {
    let dir = pen_repo_clone_dir(pen_root, pen_name, &r.owner, &r.name);
    if !dir.exists() {
        warn!(repo = %format!("{}/{}", r.owner, r.name), "clone missing; skipping");
        return Ok(RewriteRepoOutcome {
            owner: r.owner.clone(),
            repo: r.name.clone(),
            ops: Vec::new(),
            committed: false,
            rollback_trigger: None,
            logs_dir: None,
            diffs: Vec::new(),
        });
    }

    let tracked = tracked_files_in_repo(&dir, r, scan)?;
    let prior_state = read_ops_state(pen_root, pen_name, &r.owner, &r.name)?;

    // For each op, plan + apply against an in-memory map of mutated docs.
    // `staged` maps relpath → (Document, original_bytes). Across ops in
    // the same repo, the same file may be mutated multiple times.
    let mut staged: BTreeMap<String, (Document, Vec<u8>)> = BTreeMap::new();
    let mut op_outcomes: Vec<RewriteOpOutcome> = Vec::new();
    let mut rollback_trigger: Option<String> = None;
    let mut written_logs_dir: Option<PathBuf> = None;

    'op_loop: for op in ops {
        // Skip already-applied ops unless --force.
        if !options.force && prior_state.ops.contains_key(&op.id) {
            op_outcomes.push(RewriteOpOutcome {
                op_id: op.id.clone(),
                outcome: OpOutcome::NoTargets,
                files: Vec::new(),
                addresses: Vec::new(),
            });
            if !options.dry_run && !options.diff {
                let _ = append_run_log(
                    pen_root,
                    pen_name,
                    &r.owner,
                    &r.name,
                    RunLogEntry {
                        run_id: run_id.to_string(),
                        op_id: op.id.clone(),
                        ts: OffsetDateTime::now_utc(),
                        outcome: RunOutcome::Skipped,
                        files: vec![],
                        addresses: vec![],
                        reason: Some("already applied; pass --force to re-run".into()),
                        logs_dir: None,
                    },
                );
            }
            continue;
        }

        let mut files_touched: Vec<String> = Vec::new();
        let mut all_addresses: Vec<String> = Vec::new();
        let mut op_failed: Option<String> = None;

        for tf in &tracked {
            // Get or load the staged doc for this file.
            if !staged.contains_key(&tf.relpath) {
                let bytes = match std::fs::read(&tf.abspath) {
                    Ok(b) => b,
                    Err(e) => {
                        op_failed = Some(format!("read {}: {e}", tf.relpath));
                        break;
                    }
                };
                let Some(fmt) = infer_format(&tf.relpath) else {
                    continue;
                };
                let doc = match parse_bytes(&bytes, fmt) {
                    Ok(d) => d,
                    Err(e) => {
                        op_failed = Some(format!("parse {}: {e}", tf.relpath));
                        break;
                    }
                };
                staged.insert(tf.relpath.clone(), (doc, bytes));
            }

            let (doc, _) = staged.get_mut(&tf.relpath).unwrap();
            let tree = match to_json(doc) {
                Ok(t) => t,
                Err(e) => {
                    op_failed = Some(format!("to_json {}: {e}", tf.relpath));
                    break;
                }
            };
            let plan = plan_rewrite(op, &tree)?;
            if plan.addresses.is_empty() {
                continue;
            }

            // Apply deletes/sets in reverse-sorted order so array-element
            // mutations at the same depth don't shift sibling addresses
            // out from under us. Lexicographic descending approximates
            // reverse-index ordering for typical shapes.
            let mut addrs = plan.addresses.clone();
            addrs.sort_by(|a, b| b.cmp(a));

            for addr in &addrs {
                if let Err(e) = apply_at(doc, addr, &op.action) {
                    op_failed = Some(format!("{} @ {addr}: {e}", tf.relpath));
                    break;
                }
            }
            if op_failed.is_some() {
                break;
            }
            files_touched.push(tf.relpath.clone());
            all_addresses.extend(addrs);
        }

        if let Some(reason) = op_failed {
            op_outcomes.push(RewriteOpOutcome {
                op_id: op.id.clone(),
                outcome: OpOutcome::Failed {
                    reason: reason.clone(),
                },
                files: files_touched.clone(),
                addresses: all_addresses.clone(),
            });
            rollback_trigger = Some(op.id.clone());

            if !options.dry_run && !options.diff {
                let logs = write_op_logs(
                    pen_root,
                    pen_name,
                    &r.owner,
                    &r.name,
                    run_id,
                    &op.id,
                    &OpLogArtefacts { stdout: "", stderr: "", err: &reason },
                )?;
                written_logs_dir = Some(logs.clone());
                let outcome = if options.no_rollback {
                    RunOutcome::FailedNoRollback
                } else {
                    RunOutcome::RolledBack
                };
                append_run_log(
                    pen_root,
                    pen_name,
                    &r.owner,
                    &r.name,
                    RunLogEntry {
                        run_id: run_id.to_string(),
                        op_id: op.id.clone(),
                        ts: OffsetDateTime::now_utc(),
                        outcome,
                        files: files_touched,
                        addresses: all_addresses,
                        reason: Some(reason),
                        logs_dir: Some(logs.display().to_string()),
                    },
                )?;
            }
            break 'op_loop;
        }

        let outcome = if files_touched.is_empty() {
            OpOutcome::NoTargets
        } else {
            OpOutcome::Applied
        };
        op_outcomes.push(RewriteOpOutcome {
            op_id: op.id.clone(),
            outcome,
            files: files_touched,
            addresses: all_addresses,
        });
    }

    // Post-mutation schema validation, before commit.
    if let Some(reg) = registry.as_ref().filter(|_| rollback_trigger.is_none()) {
        for (path, (doc, _)) in &staged {
            let Some(schema_id) = schema_for_path(path) else {
                continue;
            };
            let json = match to_json(doc) {
                Ok(v) => v,
                Err(e) => {
                    rollback_trigger = Some(format!("to_json {path}: {e}"));
                    break;
                }
            };
            match reg.validate(schema_id, &json) {
                Ok(errs) if !errs.is_empty() => {
                    let outcome_idx = op_outcomes.iter().rposition(|o| {
                        matches!(o.outcome, OpOutcome::Applied) && o.files.contains(path)
                    });
                    let trigger_id = if let Some(i) = outcome_idx {
                        op_outcomes[i].outcome = OpOutcome::ValidationFailed {
                            errors: errs.clone(),
                        };
                        op_outcomes[i].op_id.clone()
                    } else {
                        format!("validation@{path}")
                    };
                    rollback_trigger = Some(trigger_id.clone());
                    if !options.dry_run && !options.diff {
                        let logs = write_op_logs(
                            pen_root,
                            pen_name,
                            &r.owner,
                            &r.name,
                            run_id,
                            &trigger_id,
                            &OpLogArtefacts { stdout: "", stderr: "", err: &errs.join("\n") },
                        )?;
                        written_logs_dir = Some(logs.clone());
                        let outcome = if options.no_rollback {
                            RunOutcome::FailedNoRollback
                        } else {
                            RunOutcome::RolledBack
                        };
                        append_run_log(
                            pen_root,
                            pen_name,
                            &r.owner,
                            &r.name,
                            RunLogEntry {
                                run_id: run_id.to_string(),
                                op_id: trigger_id,
                                ts: OffsetDateTime::now_utc(),
                                outcome,
                                files: vec![path.clone()],
                                addresses: vec![],
                                reason: Some(format!("schema validation: {} errors", errs.len())),
                                logs_dir: Some(logs.display().to_string()),
                            },
                        )?;
                    }
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    rollback_trigger = Some(format!("validator@{path}: {e}"));
                    break;
                }
            }
        }
    }

    // Compute diffs if asked. Independent of write decision: --diff
    // still produces a diff even when the rewrite would have rolled back.
    let mut diffs: Vec<FileDiff> = Vec::new();
    if options.diff {
        for (path, (doc, original)) in &staged {
            let new_bytes = match render(doc) {
                Ok(s) => s.into_bytes(),
                Err(_) => continue,
            };
            let original_text = String::from_utf8_lossy(original).into_owned();
            let new_text = String::from_utf8_lossy(&new_bytes).into_owned();
            let diff = similar::TextDiff::from_lines(&original_text, &new_text)
                .unified_diff()
                .header(path, path)
                .to_string();
            if !diff.is_empty() {
                diffs.push(FileDiff {
                    path: path.clone(),
                    diff,
                });
            }
        }
    }

    let committed = rollback_trigger.is_none();
    let writes_allowed = !options.dry_run && !options.diff;

    if writes_allowed {
        let proceed_with_writes = committed || options.no_rollback;
        if proceed_with_writes {
            for (path, (doc, _)) in &staged {
                let new_bytes = match render(doc) {
                    Ok(s) => s.into_bytes(),
                    Err(e) => {
                        warn!(repo = %format!("{}/{}", r.owner, r.name), %path, "render failed: {e}");
                        continue;
                    }
                };
                let abs = dir.join(path);
                if let Err(e) = std::fs::write(&abs, &new_bytes) {
                    warn!(repo = %format!("{}/{}", r.owner, r.name), %path, "write failed: {e}");
                }
            }

            // Update per-repo state and append run log entries.
            let mut state = prior_state.clone();
            let now = OffsetDateTime::now_utc();
            for o in &op_outcomes {
                match &o.outcome {
                    OpOutcome::Applied => {
                        state
                            .ops
                            .insert(o.op_id.clone(), AppliedRecord { applied_at: now });
                        state.failed.remove(&o.op_id);
                        append_run_log(
                            pen_root,
                            pen_name,
                            &r.owner,
                            &r.name,
                            RunLogEntry {
                                run_id: run_id.to_string(),
                                op_id: o.op_id.clone(),
                                ts: now,
                                outcome: RunOutcome::Applied,
                                files: o.files.clone(),
                                addresses: o.addresses.clone(),
                                reason: None,
                                logs_dir: None,
                            },
                        )?;
                    }
                    OpOutcome::NoTargets => {
                        append_run_log(
                            pen_root,
                            pen_name,
                            &r.owner,
                            &r.name,
                            RunLogEntry {
                                run_id: run_id.to_string(),
                                op_id: o.op_id.clone(),
                                ts: now,
                                outcome: RunOutcome::NoTargets,
                                files: vec![],
                                addresses: vec![],
                                reason: None,
                                logs_dir: None,
                            },
                        )?;
                    }
                    OpOutcome::Failed { reason } if options.no_rollback => {
                        state.failed.insert(
                            o.op_id.clone(),
                            FailedRecord {
                                failed_at: now,
                                reason: reason.clone(),
                            },
                        );
                    }
                    OpOutcome::ValidationFailed { errors } if options.no_rollback => {
                        state.failed.insert(
                            o.op_id.clone(),
                            FailedRecord {
                                failed_at: now,
                                reason: errors.join("\n"),
                            },
                        );
                    }
                    OpOutcome::Failed { .. } | OpOutcome::ValidationFailed { .. } => {
                        // Default rollback path: failure already recorded in
                        // run-log above; nothing to write to ops.toml.
                    }
                }
            }
            write_ops_state(pen_root, pen_name, &r.owner, &r.name, &state)?;
            if committed {
                info!(repo = %format!("{}/{}", r.owner, r.name), "rewrite committed");
            } else {
                info!(repo = %format!("{}/{}", r.owner, r.name), "rewrite committed partial (--no-rollback)");
            }
        } else {
            info!(repo = %format!("{}/{}", r.owner, r.name), "rewrite rolled back");
        }
    }

    Ok(RewriteRepoOutcome {
        owner: r.owner.clone(),
        repo: r.name.clone(),
        ops: op_outcomes,
        committed: writes_allowed && committed,
        rollback_trigger,
        logs_dir: written_logs_dir,
        diffs,
    })
}

fn aggregate_op_statuses(
    pen_root: &Path,
    pen_name: &str,
    ops: &[RewriteOp],
    repos: &[PenRepo],
) -> Result<BTreeMap<String, OpStatus>> {
    let mut out = BTreeMap::new();
    for op in ops {
        let mut applied = 0usize;
        let mut absent = 0usize;
        let mut failed = 0usize;
        for r in repos {
            let state = read_ops_state(pen_root, pen_name, &r.owner, &r.name)?;
            if state.failed.contains_key(&op.id) {
                failed += 1;
            } else if state.ops.contains_key(&op.id) {
                applied += 1;
            } else {
                absent += 1;
            }
        }
        let status = if failed > 0 {
            OpStatus::Failed
        } else if absent == repos.len() {
            OpStatus::Pending
        } else if applied == repos.len() {
            OpStatus::Applied
        } else {
            OpStatus::Partial
        };
        out.insert(op.id.clone(), status);
    }
    Ok(out)
}

fn repo_matches_only(r: &PenRepo, only: Option<&str>) -> bool {
    match only {
        None => true,
        Some(name) => {
            let full = format!("{}/{}", r.owner, r.name);
            full == name || r.name == name
        }
    }
}

fn infer_format(path: &str) -> Option<nave_parse::Format> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    match ext.to_ascii_lowercase().as_str() {
        "toml" => Some(nave_parse::Format::Toml),
        "yml" | "yaml" => Some(nave_parse::Format::Yaml),
        _ => None,
    }
}

async fn working_tree_clean(dir: &Path) -> Result<bool> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["status", "--porcelain"])
        .output()
        .await
        .context("git status")?;
    Ok(out.status.success() && out.stdout.iter().all(|b| *b == b'\n' || *b == b' '))
}

#[allow(dead_code)]
fn unused_marker(_t: &TrackedFile) {}
