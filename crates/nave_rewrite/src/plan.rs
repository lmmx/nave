//! Resolve a `RewriteOp` against a parsed tree to a list of concrete
//! addresses ready for `apply_at`.

use anyhow::{Context, Result};
use serde_json::Value;

use nave_config::{MatchPredicate, find_match_addresses};

use crate::op::{RewriteOp, Selector};

/// A planned rewrite: the op plus the concrete addresses it resolves to.
#[derive(Debug, Clone)]
pub struct PlannedRewrite<'a> {
    pub op: &'a RewriteOp,
    pub addresses: Vec<String>,
}

impl PlannedRewrite<'_> {
    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }
}

/// Resolve an op against a parsed tree. Pure function, no side effects.
pub fn plan_rewrite<'a>(op: &'a RewriteOp, tree: &Value) -> Result<PlannedRewrite<'a>> {
    let addresses = match &op.selector {
        Selector::Predicate { predicate } => {
            let pred = MatchPredicate::parse(predicate)
                .with_context(|| format!("parsing selector predicate {predicate:?}"))?;
            find_match_addresses(tree, &pred)
        }
    };
    Ok(PlannedRewrite { op, addresses })
}

/// Convenience: resolve every op against the same tree, returning a
/// vector of plans in op order. Errors propagate from the first
/// malformed selector encountered.
pub fn plan_all<'a>(ops: &'a [RewriteOp], tree: &Value) -> Result<Vec<PlannedRewrite<'a>>> {
    ops.iter().map(|op| plan_rewrite(op, tree)).collect()
}
