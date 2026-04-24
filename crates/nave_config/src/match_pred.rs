//! Structural match predicates over a parsed tree.
//!
//! A predicate has the form `<relative-path> <op> <literal>`, where
//! `<op>` is one of:
//!
//!   =   exact string equality against the scalar's rendered form
//!   !=  negated exact equality
//!   ^=  starts-with
//!   $=  ends-with
//!   *=  contains (substring)
//!
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
    /// Negated exact equality.
    NotEq,
    /// Starts-with match against the scalar's rendered form.
    StartsWith,
    /// Ends-with match against the scalar's rendered form.
    EndsWith,
    /// Substring match against the scalar's rendered form.
    Contains,
}

impl Op {
    /// Render as the source text the parser accepts. Useful for
    /// error messages and round-tripping.
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::NotEq => "!=",
            Op::StartsWith => "^=",
            Op::EndsWith => "$=",
            Op::Contains => "*=",
        }
    }
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
    ///   op := "=" | "!=" | "^=" | "$=" | "*="
    ///
    /// Whitespace around `op` is optional. The literal runs to end of
    /// string (after stripping surrounding quotes if present).
    pub fn parse(input: &str) -> Result<Self> {
        let raw = input.to_string();

        // Peel off scope (unquoted `:` before the op).
        let (scope, rest) = split_scope(input);

        // Find the op — first unquoted op in `rest`, preferring longer matches.
        let (lhs, op, rhs) = split_op(rest).ok_or_else(|| {
            anyhow!(
                "match predicate missing operator (one of `=`, `!=`, `^=`, `$=`, `*=`): {input:?}"
            )
        })?;

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
            Op::NotEq => rendered != pred.literal,
            Op::StartsWith => rendered.starts_with(&pred.literal),
            Op::EndsWith => rendered.ends_with(&pred.literal),
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
                // Don't treat a `:` as scope if the op-bearing chars
                // have already appeared — that means there's no scope.
                if scope.is_empty()
                    || scope.contains('=')
                    || scope.contains('!')
                    || scope.contains('^')
                    || scope.contains('$')
                    || scope.contains('*')
                {
                    return (None, input);
                }
                return (Some(scope.to_string()), rest);
            }
            _ => {}
        }
    }
    (None, input)
}

/// Scan `input` for the first unquoted operator. Two-character ops
/// (`!=`, `^=`, `$=`, `*=`) are checked before the single-char `=`
/// so that e.g. `foo != bar` doesn't parse as `foo ! ` + `=` + ` bar`.
fn split_op(input: &str) -> Option<(&str, Op, &str)> {
    let bytes = input.as_bytes();
    let mut in_quotes = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if in_quotes {
            i += 1;
            continue;
        }
        // Two-char ops first.
        if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
            let op = match b {
                b'!' => Some(Op::NotEq),
                b'^' => Some(Op::StartsWith),
                b'$' => Some(Op::EndsWith),
                b'*' => Some(Op::Contains),
                _ => None,
            };
            if let Some(op) = op {
                return Some((&input[..i], op, &input[i + 2..]));
            }
        }
        // Single-char op.
        if b == b'=' {
            return Some((&input[..i], Op::Eq, &input[i + 1..]));
        }
        i += 1;
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

    // --- parser ---

    #[test]
    fn parse_eq() {
        let p = MatchPredicate::parse("with.command = upload").unwrap();
        assert_eq!(p.scope, None);
        assert_eq!(p.rel_path, "with.command");
        assert_eq!(p.op, Op::Eq);
        assert_eq!(p.literal, "upload");
    }

    #[test]
    fn parse_not_eq() {
        let p = MatchPredicate::parse("with.command != upload").unwrap();
        assert_eq!(p.op, Op::NotEq);
        assert_eq!(p.literal, "upload");
    }

    #[test]
    fn parse_starts_with() {
        let p = MatchPredicate::parse("uses ^= PyO3/maturin-action@").unwrap();
        assert_eq!(p.op, Op::StartsWith);
        assert_eq!(p.literal, "PyO3/maturin-action@");
    }

    #[test]
    fn parse_ends_with() {
        let p = MatchPredicate::parse("path $= .yml").unwrap();
        assert_eq!(p.op, Op::EndsWith);
        assert_eq!(p.literal, ".yml");
    }

    #[test]
    fn parse_contains() {
        let p = MatchPredicate::parse("uses *= maturin").unwrap();
        assert_eq!(p.op, Op::Contains);
        assert_eq!(p.literal, "maturin");
    }

    #[test]
    fn parse_no_spaces_around_op() {
        let p = MatchPredicate::parse("uses^=PyO3/").unwrap();
        assert_eq!(p.op, Op::StartsWith);
        assert_eq!(p.rel_path, "uses");
        assert_eq!(p.literal, "PyO3/");
    }

    #[test]
    fn parse_with_scope() {
        let p = MatchPredicate::parse("workflow:with.command = upload").unwrap();
        assert_eq!(p.scope.as_deref(), Some("workflow"));
        assert_eq!(p.rel_path, "with.command");
        assert_eq!(p.op, Op::Eq);
    }

    #[test]
    fn parse_scope_with_two_char_op() {
        let p = MatchPredicate::parse("workflow:uses ^= PyO3/").unwrap();
        assert_eq!(p.scope.as_deref(), Some("workflow"));
        assert_eq!(p.op, Op::StartsWith);
        assert_eq!(p.literal, "PyO3/");
    }

    #[test]
    fn parse_quoted_literal() {
        let p = MatchPredicate::parse(r#"name = "Publish to PyPI""#).unwrap();
        assert_eq!(p.op, Op::Eq);
        assert_eq!(p.literal, "Publish to PyPI");
    }

    #[test]
    fn parse_quoted_literal_with_op_char_inside() {
        // The `=` inside quotes must not be taken as the operator.
        let p = MatchPredicate::parse(r#"uses *= "a=b""#).unwrap();
        assert_eq!(p.op, Op::Contains);
        assert_eq!(p.literal, "a=b");
    }

    #[test]
    fn parse_missing_op_is_error() {
        let err = MatchPredicate::parse("just.a.path").unwrap_err();
        assert!(format!("{err}").contains("missing operator"));
    }

    // --- evaluation ---

    fn step_tree() -> Value {
        json!({
            "jobs": {
                "release": {
                    "steps": [
                        {"uses": "actions/checkout@v4"},
                        {
                            "uses": "PyO3/maturin-action@v1",
                            "with": {"command": "upload", "args": "--skip-existing"}
                        },
                        {
                            "uses": "pypa/gh-action-pypi-publish@release/v1",
                            "with": {"packages-dir": "dist/"}
                        }
                    ]
                }
            }
        })
    }

    #[test]
    fn eval_eq_hits_exact_match() {
        let tree = step_tree();
        let p = MatchPredicate::parse("with.command = upload").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(addrs, vec!["jobs.release.steps[1]".to_string()]);
    }

    #[test]
    fn eval_not_eq_excludes_match_but_includes_others_with_field() {
        // `!=` matches every anchor where `with.command` exists and isn't "upload".
        // Only steps[1] has `with.command` at all, and it equals "upload", so no hits.
        let tree = step_tree();
        let p = MatchPredicate::parse("with.command != upload").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert!(addrs.is_empty(), "got: {addrs:?}");
    }

    #[test]
    fn eval_not_eq_finds_anchors_with_differing_value() {
        let tree = json!({
            "steps": [
                {"with": {"command": "upload"}},
                {"with": {"command": "build"}},
                {"with": {"command": "sdist"}},
            ]
        });
        let p = MatchPredicate::parse("with.command != upload").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        // steps[1] and steps[2] qualify. The outer `steps[1]`/`steps[2]`
        // objects are the tightest anchors at which with.command resolves.
        assert!(addrs.contains(&"steps[1]".to_string()));
        assert!(addrs.contains(&"steps[2]".to_string()));
        assert!(!addrs.contains(&"steps[0]".to_string()));
    }

    #[test]
    fn eval_starts_with() {
        let tree = step_tree();
        let p = MatchPredicate::parse("uses ^= PyO3/").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(addrs, vec!["jobs.release.steps[1]".to_string()]);
    }

    #[test]
    fn eval_ends_with() {
        let tree = step_tree();
        let p = MatchPredicate::parse("uses $= @v4").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(addrs, vec!["jobs.release.steps[0]".to_string()]);
    }

    #[test]
    fn eval_contains() {
        let tree = step_tree();
        let p = MatchPredicate::parse("uses *= maturin").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(addrs, vec!["jobs.release.steps[1]".to_string()]);
    }

    #[test]
    fn eval_contains_misses_when_absent() {
        let tree = step_tree();
        let p = MatchPredicate::parse("uses *= zzz-nothing").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert!(addrs.is_empty());
    }
}
