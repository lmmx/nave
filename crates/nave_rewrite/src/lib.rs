//! Declarative rewrites over parsed config trees.
//!
//! Pure library: given parsed bytes and a list of ops, produces mutated
//! bytes. No I/O, no pen awareness, no schema awareness. Orchestration
//! (state files, dirty-tree gating, parallelism) lives in `nave_pen`.
//!
//! Two-phase model:
//!   1. `plan_rewrite` resolves a `RewriteOp` against a parsed tree to a
//!      list of concrete addresses.
//!   2. `apply_at` mutates a parsed `Document` in place at a given
//!      address according to an `Action`.
//!
//! Callers stage all mutations in memory before writing, so per-repo
//! atomicity (rollback on failure) is the orchestrator's responsibility,
//! not this crate's.

pub mod apply;
pub mod op;
pub mod plan;
pub mod report;

pub use apply::{ApplyError, apply_at};
pub use op::{Action, OpStatus, RewriteOp, Selector};
pub use plan::{PlannedRewrite, plan_rewrite};
pub use report::{FileOutcome, OpOutcome, RewriteOutcome};
