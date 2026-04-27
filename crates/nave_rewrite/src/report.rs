//! Per-file and per-op outcomes returned by the rewrite pipeline.
//!
//! The orchestrator aggregates these into per-repo state and the
//! pen-level run log. This crate is unaware of where they go.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RewriteOutcome {
    pub op_id: String,
    pub files: Vec<FileOutcome>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileOutcome {
    pub path: String,
    pub addresses: Vec<String>,
    pub status: OpOutcome,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpOutcome {
    /// Op was applied: addresses were mutated and the rendered bytes are ready.
    Applied,
    /// Selector resolved to zero addresses; nothing to do.
    NoTargets,
    /// One or more addresses failed to apply. Reason is the first error.
    Failed { reason: String },
    /// Validation rejected the post-rewrite document.
    ValidationFailed { errors: Vec<String> },
}

impl OpOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, OpOutcome::Applied | OpOutcome::NoTargets)
    }
}
