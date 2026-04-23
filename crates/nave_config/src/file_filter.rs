//! Shared file-filtering logic used by `nave search --match` and `nave build --match`.
//!
//! A filter is (where-terms, match-predicates). A file passes the filter iff:
//!   - for every term whose scope applies to the file's pattern,
//!     at least one of its needles has a hit in the parsed tree;
//!   - for every predicate whose scope applies to the file's pattern,
//!     at least one address satisfies it in the parsed tree.
//!
//! "Applies to the file's pattern" uses the same substring-scope semantics
//! as `Term::applies_to_pattern` / `MatchPredicate::applies_to_pattern`.

use serde_json::Value;

use crate::address::find_addresses;
use crate::match_pred::{MatchPredicate, find_match_addresses};
use crate::term::Term;

#[derive(Debug, Default)]
pub struct FileFilter<'a> {
    pub where_terms: &'a [Term],
    pub match_preds: &'a [MatchPredicate],
}

impl FileFilter<'_> {
    pub fn is_empty(&self) -> bool {
        self.where_terms.is_empty() && self.match_preds.is_empty()
    }

    /// Does this filter apply any constraint to files matching `pattern`?
    /// If not, evaluation can be skipped entirely.
    pub fn applies_to_pattern(&self, pattern: &str) -> bool {
        self.where_terms
            .iter()
            .any(|t| t.applies_to_pattern(pattern))
            || self
                .match_preds
                .iter()
                .any(|p| p.applies_to_pattern(pattern))
    }

    /// Evaluate the filter against a parsed tree. A term/predicate whose
    /// scope doesn't apply to the file's pattern is considered satisfied.
    pub fn evaluate(&self, pattern: &str, tree: &Value) -> bool {
        for t in self.where_terms {
            if !t.applies_to_pattern(pattern) {
                continue;
            }
            let hit = t
                .needles
                .iter()
                .any(|needle| !find_addresses(tree, needle).is_empty());
            if !hit {
                return false;
            }
        }
        for p in self.match_preds {
            if !p.applies_to_pattern(pattern) {
                continue;
            }
            if find_match_addresses(tree, p).is_empty() {
                return false;
            }
        }
        true
    }
}
