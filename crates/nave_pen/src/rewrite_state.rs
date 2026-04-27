//! Per-repo rewrite state.
//!
//! Distinct from the per-repo *git* state in `crate::state` — that
//! module tracks working-tree cleanliness, freshness vs the cache,
//! and divergence vs origin. This module tracks which declarative
//! rewrite ops (from `pen.toml`'s `[[ops]]`) have been applied to
//! which repos, plus an append-only run log and per-failure log
//! artefacts.
//!
//! Layout under a pen:
//! ```text
//! <pen_dir>/state/<owner>__<repo>/
//!   ops.toml         live state: which ops have applied here
//!   run-log.toml     append-only history of rewrite attempts
//!   logs/<run-id>/<op-id>.{stdout,stderr,err}
//! ```
//!
//! Workers writing to `state/<owner>__<repo>/` are guaranteed disjoint;
//! no locking required for parallelism. Aggregation into `pen.toml`
//! happens once at run end on the orchestrator.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::storage::pen_dir;

/// `state/` directory for an entire pen.
pub fn pen_state_dir(pen_root: &Path, pen_name: &str) -> PathBuf {
    pen_dir(pen_root, pen_name).join("state")
}

/// Per-repo `state/<owner>__<repo>/` directory.
pub fn repo_state_dir(pen_root: &Path, pen_name: &str, owner: &str, repo: &str) -> PathBuf {
    pen_state_dir(pen_root, pen_name).join(format!("{owner}__{repo}"))
}

pub fn ops_toml_path(pen_root: &Path, pen_name: &str, owner: &str, repo: &str) -> PathBuf {
    repo_state_dir(pen_root, pen_name, owner, repo).join("ops.toml")
}

pub fn run_log_path(pen_root: &Path, pen_name: &str, owner: &str, repo: &str) -> PathBuf {
    repo_state_dir(pen_root, pen_name, owner, repo).join("run-log.toml")
}

pub fn logs_dir(pen_root: &Path, pen_name: &str, owner: &str, repo: &str, run_id: &str) -> PathBuf {
    repo_state_dir(pen_root, pen_name, owner, repo)
        .join("logs")
        .join(run_id)
}

/// `ops.toml` — live state of which ops have applied to this repo.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoOpsState {
    /// op id → record. Presence = applied to this repo.
    #[serde(default)]
    pub ops: BTreeMap<String, AppliedRecord>,
    /// op id → record. Only populated under `--no-rollback` failures.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub failed: BTreeMap<String, FailedRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedRecord {
    #[serde(with = "time::serde::rfc3339")]
    pub applied_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedRecord {
    #[serde(with = "time::serde::rfc3339")]
    pub failed_at: OffsetDateTime,
    pub reason: String,
}

pub fn read_ops_state(
    pen_root: &Path,
    pen_name: &str,
    owner: &str,
    repo: &str,
) -> Result<RepoOpsState> {
    let path = ops_toml_path(pen_root, pen_name, owner, repo);
    if !path.exists() {
        return Ok(RepoOpsState::default());
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let state = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(state)
}

pub fn write_ops_state(
    pen_root: &Path,
    pen_name: &str,
    owner: &str,
    repo: &str,
    state: &RepoOpsState,
) -> Result<()> {
    let dir = repo_state_dir(pen_root, pen_name, owner, repo);
    std::fs::create_dir_all(&dir)?;
    let text = toml::to_string_pretty(state)?;
    let path = ops_toml_path(pen_root, pen_name, owner, repo);
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// `run-log.toml` — append-only history.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoRunLog {
    #[serde(rename = "entry", default)]
    pub entries: Vec<RunLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLogEntry {
    pub run_id: String,
    pub op_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub ts: OffsetDateTime,
    pub outcome: RunOutcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Absolute path to the per-run log directory if logs were written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs_dir: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    /// Successfully applied.
    Applied,
    /// Selector resolved to nothing.
    NoTargets,
    /// Failed but rolled back; working tree untouched.
    RolledBack,
    /// Failed under `--no-rollback`; working tree may have partial changes.
    FailedNoRollback,
    /// Op was skipped because it was already applied (no `--force`).
    Skipped,
}

pub fn append_run_log(
    pen_root: &Path,
    pen_name: &str,
    owner: &str,
    repo: &str,
    entry: RunLogEntry,
) -> Result<()> {
    let dir = repo_state_dir(pen_root, pen_name, owner, repo);
    std::fs::create_dir_all(&dir)?;
    let path = run_log_path(pen_root, pen_name, owner, repo);
    let mut log: RepoRunLog = if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).unwrap_or_default()
    } else {
        RepoRunLog::default()
    };
    log.entries.push(entry);
    let text = toml::to_string_pretty(&log)?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Generate a run id from the current UTC time, suitable for use as a
/// filesystem path component.
pub fn new_run_id() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}

/// Bundle of log artefacts to write for a single op invocation.
/// Empty strings produce empty files (kept for layout consistency).
#[derive(Debug, Default, Clone)]
pub struct OpLogArtefacts<'a> {
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub err: &'a str,
}

pub fn write_op_logs(
    pen_root: &Path,
    pen_name: &str,
    owner: &str,
    repo: &str,
    run_id: &str,
    op_id: &str,
    artefacts: &OpLogArtefacts<'_>,
) -> Result<PathBuf> {
    let dir = logs_dir(pen_root, pen_name, owner, repo, run_id);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{op_id}.stdout")), artefacts.stdout)?;
    std::fs::write(dir.join(format!("{op_id}.stderr")), artefacts.stderr)?;
    std::fs::write(dir.join(format!("{op_id}.err")), artefacts.err)?;
    Ok(dir)
}
