//! Structural match predicates over a parsed tree.
//!
//! A predicate has the form `<relative-path> <op> <literal>`, where
//! `<op>` is `=` (exact string equality) or `~` (substring contains).
//! Relative paths use the same dot/bracket syntax as tree addresses
//! elsewhere, e.g. `with.command`, `steps[0].uses`.
//!
//! A predicate "matches at address A in tree T" when resolving
//! `<relative-path>` under the subtree at A yields a scalar value
//! whose string form satisfies `<op> <literal>`.
//!
//! Given a tree, `find_match_addresses` returns every address where
//! the predicate matches — these addresses plug into the same
//! co-occurrence logic as `--where` hits.

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::address::{find_addresses_all_objects, subtree_at};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Exact string equality against the scalar's rendered form.
    Eq,
    /// Substring match against the scalar's rendered form.
    Contains,
}

#[derive(Debug, Clone)]
pub struct MatchPredicate {
    /// Optional scope restricting which tracked-path patterns this
    /// predicate applies to — same semantics as `Term.scope`.
    pub scope: Option<String>,
    /// Relative path from a candidate anchor address to the scalar
    /// being tested. Empty means "the candidate itself".
    pub rel_path: String,
    pub op: Op,
    pub literal: String,
    /// Original text, for display.
    pub raw: String,
}

impl MatchPredicate {
    /// Parse a `--match` argument. Grammar:
    ///
    ///   [scope ":"] path op literal
    ///   op := "=" | "~"
    ///
    /// Whitespace around `op` is optional. The literal runs to end of
    /// string (after stripping surrounding quotes if present).
    pub fn parse(input: &str) -> Result<Self> {
        let raw = input.to_string();

        // Peel off scope (unquoted `:` before the op).
        let (scope, rest) = split_scope(input);

        // Find the op — first unquoted `=` or `~` in `rest`.
        let (lhs, op, rhs) = split_op(rest)
            .ok_or_else(|| anyhow!("match predicate missing operator (`=` or `~`): {input:?}"))?;

        let rel_path = lhs.trim().to_string();
        let literal = unquote(rhs.trim()).to_string();

        Ok(Self {
            scope,
            rel_path,
            op,
            literal,
            raw,
        })
    }

    pub fn applies_to_pattern(&self, pattern: &str) -> bool {
        match &self.scope {
            None => true,
            Some(s) => pattern.contains(s.as_str()),
        }
    }
}

/// Find every address in `tree` where `pred` matches. For each object
/// node in the tree (including the root), resolve `pred.rel_path` under
/// it; if the result is a scalar satisfying `pred.op`/`pred.literal`,
/// emit the object's address.
pub fn find_match_addresses(tree: &Value, pred: &MatchPredicate) -> Vec<String> {
    let mut out = Vec::new();
    for anchor_addr in find_addresses_all_objects(tree) {
        let Some(anchor) = subtree_at(tree, &anchor_addr) else {
            continue;
        };
        let target = if pred.rel_path.is_empty() {
            Some(anchor.clone())
        } else {
            subtree_at(&anchor, &pred.rel_path)
        };
        let Some(value) = target else { continue };
        let rendered = match &value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => continue, // non-scalar: predicate doesn't apply
        };
        let matches = match pred.op {
            Op::Eq => rendered == pred.literal,
            Op::Contains => rendered.contains(&pred.literal),
        };
        if matches {
            out.push(anchor_addr);
        }
    }
    out
}

// --- parser helpers ---

fn split_scope(input: &str) -> (Option<String>, &str) {
    let bytes = input.as_bytes();
    let mut in_quotes = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quotes = !in_quotes,
            b':' if !in_quotes => {
                let scope = &input[..i];
                let rest = &input[i + 1..];
                if scope.is_empty() || scope.contains('=') || scope.contains('~') {
                    return (None, input);
                }
                return (Some(scope.to_string()), rest);
            }
            _ => {}
        }
    }
    (None, input)
}

fn split_op(input: &str) -> Option<(&str, Op, &str)> {
    let bytes = input.as_bytes();
    let mut in_quotes = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quotes = !in_quotes,
            b'=' if !in_quotes => {
                return Some((&input[..i], Op::Eq, &input[i + 1..]));
            }
            b'~' if !in_quotes => {
                return Some((&input[..i], Op::Contains, &input[i + 1..]));
            }
            _ => {}
        }
    }
    None
}

fn unquote(s: &str) -> &str {
    let b = s.as_bytes();
    if b.len() >= 2 && b[0] == b'"' && b[b.len() - 1] == b'"' {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_basic() {
        let p = MatchPredicate::parse("with.command = upload").unwrap();
        assert_eq!(p.scope, None);
        assert_eq!(p.rel_path, "with.command");
        assert_eq!(p.op, Op::Eq);
        assert_eq!(p.literal, "upload");
    }

    #[test]
    fn parse_with_scope() {
        let p = MatchPredicate::parse("workflow:with.command = upload").unwrap();
        assert_eq!(p.scope.as_deref(), Some("workflow"));
        assert_eq!(p.rel_path, "with.command");
    }

    #[test]
    fn parse_contains_op() {
        let p = MatchPredicate::parse("uses ~ maturin-action").unwrap();
        assert_eq!(p.op, Op::Contains);
        assert_eq!(p.literal, "maturin-action");
    }

    #[test]
    fn parse_quoted_literal() {
        let p = MatchPredicate::parse(r#"name = "Publish to PyPI""#).unwrap();
        assert_eq!(p.literal, "Publish to PyPI");
    }

    #[test]
    fn finds_step_with_command_upload() {
        let tree = json!({
            "jobs": {
                "release": {
                    "steps": [
                        {"uses": "actions/checkout@v4"},
                        {
                            "uses": "PyO3/maturin-action@v1",
                            "with": {"command": "upload"}
                        }
                    ]
                }
            }
        });
        let pred = MatchPredicate::parse("with.command = upload").unwrap();
        let addrs = find_match_addresses(&tree, &pred);
        assert!(addrs.contains(&"jobs.release.steps[1]".to_string()));
        // Also matches at the step object's enclosing ancestors if their
        // `with.command` resolves — but `jobs.release.steps[1]` is the
        // tightest. Higher ancestors don't have `with.command` directly.
        assert_eq!(addrs.len(), 1);
    }
}
