//! The rewrite IR: ops, selectors, actions.
//!
//! Serialised form lives in `pen.toml` under `[[ops]]`. The `status`
//! field is the pen-level aggregate; per-repo state is tracked
//! separately (see `nave_pen::state`).

use serde::{Deserialize, Serialize};

/// A single declarative rewrite operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewriteOp {
    /// Stable identifier for this op. Required, user-supplied at pen
    /// creation time. Survives op reordering in `pen.toml`.
    pub id: String,
    /// What to target.
    pub selector: Selector,
    /// What to do at each target.
    pub action: Action,
    /// Aggregate status across all in-scope repos.
    #[serde(default)]
    pub status: OpStatus,
}

/// Pen-level aggregate status for an op.
///
/// Per-repo state lives in `state/<owner>__<repo>/ops.toml`. This field
/// is computed from per-repo state at the end of every `pen rewrite`
/// run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum OpStatus {
    /// No repo has succeeded yet, and no `--no-rollback` failures.
    #[default]
    Pending,
    /// Every in-scope repo has applied this op.
    Applied,
    /// Some repos applied, others didn't.
    Partial,
    /// At least one repo has a `--no-rollback` failure for this op.
    Failed,
}


/// What to target.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Selector {
    /// Predicate string in the same grammar as `--match`.
    Predicate { predicate: String },
}

/// What to do at each target address.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Replace the value at the matched address.
    Set { value: serde_json::Value },
    /// Delete the key (in objects) or element (in arrays) at the
    /// matched address.
    Delete,
    /// Rename the leaf key of the matched address. Only valid when the
    /// matched address ends in an object key segment.
    RenameKey { to: String },
    /// Insert a sibling key=value into the parent object of the matched
    /// address.
    InsertSibling {
        key: String,
        value: serde_json::Value,
    },
}
