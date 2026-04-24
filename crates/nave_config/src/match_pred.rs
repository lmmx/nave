//! Structural match predicates over a parsed tree.
//!
//! A predicate has the form:
//!
//!   [scope:] [!] path [op literal]
//!
//! Operators:
//!
//!   =   exact string equality against the scalar's rendered form
//!   !=  negated exact equality
//!   ^=  starts-with
//!   $=  ends-with
//!   *=  contains (substring)
//!
//! A predicate with no operator is a *presence* predicate: it matches
//! at addresses where the path resolves to at least one value. A
//! presence predicate prefixed with `!` is an *absence* predicate: it
//! matches at anchor addresses where the path resolves to zero values.
//!
//! Paths use the same dot/bracket syntax as tree addresses elsewhere.
//! `[]` is a wildcard over array elements.
//!
//! A predicate "matches at address A in tree T" when resolving the
//! path under the subtree at A yields at least one scalar value whose
//! string form satisfies the op (binary ops); or at least one value
//! at all (presence); or no values at all (absence).
//!
//! The emitted address is:
//!   - the concrete scalar's address for binary ops and presence
//!   - the anchor's address for absence (there's nothing concrete to
//!     point at for something that doesn't exist)

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::address::{
    Segment, find_addresses_all_objects, parse_address, resolve_rel_path, subtree_at,
};

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
    /// Presence: path resolves to ≥1 value.
    Present,
    /// Absence: path resolves to zero values.
    Absent,
}

impl Op {
    /// Render as the source text the parser accepts. Useful for error
    /// messages and round-tripping. Note: `Present`/`Absent` render as
    /// the empty string because they're expressed via the presence of
    /// the path itself (with an optional `!` prefix), not an infix op.
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::NotEq => "!=",
            Op::StartsWith => "^=",
            Op::EndsWith => "$=",
            Op::Contains => "*=",
            Op::Present | Op::Absent => "",
        }
    }

    /// Does this op take a literal operand?
    pub fn is_binary(self) -> bool {
        !matches!(self, Op::Present | Op::Absent)
    }
}

#[derive(Debug, Clone)]
pub struct MatchPredicate {
    /// Optional scope restricting which tracked-path patterns this
    /// predicate applies to — same semantics as `Term.scope`.
    pub scope: Option<String>,
    /// Relative path from a candidate anchor address to the scalar
    /// being tested. Never empty.
    pub rel_path: String,
    pub op: Op,
    pub literal: String,
    /// Original text, for display.
    pub raw: String,
}

impl MatchPredicate {
    /// Parse a `--match` argument. Grammar:
    ///
    ///   [scope ":"] [!] path [op literal]
    ///   op := "=" | "!=" | "^=" | "$=" | "*="
    ///
    /// With no op, the predicate is a presence check (or absence, if
    /// `!` prefixes the path).
    pub fn parse(input: &str) -> Result<Self> {
        let raw = input.to_string();

        // Peel off scope (unquoted `:` before any op or `!`).
        let (scope, rest) = split_scope(input);

        // Peel off leading `!` (absence marker).
        let (absence, after_bang) = if let Some(stripped) = rest.trim_start().strip_prefix('!') {
            // A `!` immediately followed by `=` is the start of the
            // `!=` operator on a bare-op form, not an absence marker.
            // Rare but possible: `!= foo` with no lhs would be nonsense
            // anyway, so we only treat `!` as absence when the char
            // after it isn't `=`.
            if stripped.starts_with('=') {
                (false, rest)
            } else {
                (true, stripped)
            }
        } else {
            (false, rest)
        };

        // Split at first unquoted infix op.
        if let Some((lhs, op, rhs)) = split_op(after_bang) {
            if absence {
                return Err(anyhow!(
                    "absence predicate `!path` takes no operator (got {}): {input:?}",
                    op.as_str()
                ));
            }
            let rel_path = lhs.trim().to_string();
            let literal = unquote(rhs.trim()).to_string();
            if rel_path.is_empty() {
                return Err(anyhow!("match predicate has empty path: {input:?}"));
            }
            Ok(Self {
                scope,
                rel_path,
                op,
                literal,
                raw,
            })
        } else {
            // No infix op: presence (or absence).
            let rel_path = after_bang.trim().to_string();
            if rel_path.is_empty() {
                return Err(anyhow!("match predicate has empty path: {input:?}"));
            }
            Ok(Self {
                scope,
                rel_path,
                op: if absence { Op::Absent } else { Op::Present },
                literal: String::new(),
                raw,
            })
        }
    }

    pub fn applies_to_pattern(&self, pattern: &str) -> bool {
        match &self.scope {
            None => true,
            Some(s) => pattern.contains(s.as_str()),
        }
    }

    /// Test the op against a rendered scalar. Only meaningful for
    /// binary ops; unary ops don't reach this.
    fn matches_scalar(&self, rendered: &str) -> bool {
        match self.op {
            Op::Eq => rendered == self.literal,
            Op::NotEq => rendered != self.literal,
            Op::StartsWith => rendered.starts_with(&self.literal),
            Op::EndsWith => rendered.ends_with(&self.literal),
            Op::Contains => rendered.contains(&self.literal),
            Op::Present | Op::Absent => unreachable!("unary op reached scalar comparison"),
        }
    }
}

/// Rendered string form of a scalar value, or `None` if it's not a
/// scalar.
fn render_scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Find every address in `tree` where `pred` matches.
///
/// For binary ops and `Present`, emits concrete scalar/value addresses.
/// For `Absent`, emits anchor addresses (objects or arrays that could
/// host the path but don't).
pub fn find_match_addresses(tree: &Value, pred: &MatchPredicate) -> Vec<String> {
    let mut out = Vec::new();

    for anchor_addr in find_addresses_all_objects(tree) {
        let Some(anchor) = subtree_at(tree, &anchor_addr) else {
            continue;
        };
        // Skip if rel_path is malformed
        let Ok(resolved) = resolve_rel_path(&anchor, &pred.rel_path) else {
            continue;
        };

        match pred.op {
            Op::Present => {
                for (rel_addr, _) in resolved {
                    out.push(join_address(&anchor_addr, &rel_addr));
                }
            }
            Op::Absent => {
                if resolved.is_empty() && is_host_for_path(&anchor, &pred.rel_path) {
                    out.push(anchor_addr);
                }
            }
            _ => {
                // Binary op: emit concrete address of each matching scalar.
                for (rel_addr, value) in resolved {
                    let Some(rendered) = render_scalar(&value) else {
                        continue;
                    };
                    if !pred.matches_scalar(&rendered) {
                        continue;
                    }
                    out.push(join_address(&anchor_addr, &rel_addr));
                }
            }
        }
    }

    dedup_preserve_order(out)
}

/// Would a node of this shape legitimately host the given path?
/// Used by `Absent` to avoid emitting every leaf in the tree as a
/// match. A key-first path only makes sense under an object; an
/// index-first or wildcard path only under an array.
fn is_host_for_path(anchor: &Value, path: &str) -> bool {
    let Ok(segs) = parse_address(path) else {
        return false;
    };
    match segs.first() {
        Some(Segment::Key(_)) => matches!(anchor, Value::Object(_)),
        Some(Segment::Index(_) | Segment::Any) => matches!(anchor, Value::Array(_)),
        None => false,
    }
}

fn join_address(anchor: &str, rel: &str) -> String {
    match (anchor.is_empty(), rel.is_empty()) {
        (true, _) => rel.to_string(),
        (false, true) => anchor.to_string(),
        (false, false) => {
            if rel.starts_with('[') {
                format!("{anchor}{rel}")
            } else {
                format!("{anchor}.{rel}")
            }
        }
    }
}

fn dedup_preserve_order(xs: Vec<String>) -> Vec<String> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(xs.len());
    for x in xs {
        if seen.insert(x.clone()) {
            out.push(x);
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

/// Scan `input` for the first unquoted infix operator. Two-char ops
/// (`!=`, `^=`, `$=`, `*=`) are checked before the single-char `=`.
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
        let p = MatchPredicate::parse(r#"uses *= "a=b""#).unwrap();
        assert_eq!(p.op, Op::Contains);
        assert_eq!(p.literal, "a=b");
    }

    #[test]
    fn parse_presence() {
        let p = MatchPredicate::parse("tool.maturin").unwrap();
        assert_eq!(p.op, Op::Present);
        assert_eq!(p.rel_path, "tool.maturin");
        assert_eq!(p.literal, "");
    }

    #[test]
    fn parse_presence_with_scope() {
        let p = MatchPredicate::parse("pyproject:tool.maturin").unwrap();
        assert_eq!(p.scope.as_deref(), Some("pyproject"));
        assert_eq!(p.op, Op::Present);
        assert_eq!(p.rel_path, "tool.maturin");
    }

    #[test]
    fn parse_absence() {
        let p = MatchPredicate::parse("!tool.maturin").unwrap();
        assert_eq!(p.op, Op::Absent);
        assert_eq!(p.rel_path, "tool.maturin");
    }

    #[test]
    fn parse_absence_with_scope() {
        let p = MatchPredicate::parse("workflow:!jobs.test").unwrap();
        assert_eq!(p.scope.as_deref(), Some("workflow"));
        assert_eq!(p.op, Op::Absent);
        assert_eq!(p.rel_path, "jobs.test");
    }

    #[test]
    fn parse_absence_with_op_errors() {
        let err = MatchPredicate::parse("!foo = bar").unwrap_err();
        assert!(format!("{err}").contains("takes no operator"));
    }

    #[test]
    fn parse_empty_path_errors() {
        let err = MatchPredicate::parse("").unwrap_err();
        assert!(format!("{err}").contains("empty path"));
    }

    #[test]
    fn parse_bang_only_errors() {
        let err = MatchPredicate::parse("!").unwrap_err();
        assert!(format!("{err}").contains("empty path"));
    }

    // --- evaluation: binary ops ---

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
    fn eval_eq_emits_concrete_scalar_address() {
        let tree = step_tree();
        let p = MatchPredicate::parse("with.command = upload").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(
            addrs,
            vec!["jobs.release.steps[1].with.command".to_string()]
        );
    }

    #[test]
    fn eval_starts_with_concrete_address() {
        let tree = step_tree();
        let p = MatchPredicate::parse("uses ^= PyO3/").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(addrs, vec!["jobs.release.steps[1].uses".to_string()]);
    }

    #[test]
    fn eval_ends_with_concrete_address() {
        let tree = step_tree();
        let p = MatchPredicate::parse("uses $= @v4").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(addrs, vec!["jobs.release.steps[0].uses".to_string()]);
    }

    #[test]
    fn eval_contains_concrete_address() {
        let tree = step_tree();
        let p = MatchPredicate::parse("uses *= maturin").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(addrs, vec!["jobs.release.steps[1].uses".to_string()]);
    }

    #[test]
    fn eval_contains_misses_when_absent() {
        let tree = step_tree();
        let p = MatchPredicate::parse("uses *= zzz-nothing").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert!(addrs.is_empty());
    }

    // --- evaluation: any-wildcard ---

    fn dependabot_tree() -> Value {
        json!({
            "version": 2,
            "updates": [
                {"package-ecosystem": "github-actions", "schedule": {"interval": "weekly"}},
                {"package-ecosystem": "cargo", "schedule": {"interval": "monthly"}},
                {"package-ecosystem": "pip", "schedule": {"interval": "weekly"}},
            ]
        })
    }

    #[test]
    fn eval_any_wildcard_in_rel_path() {
        let tree = dependabot_tree();
        let p = MatchPredicate::parse("updates[].schedule.interval = weekly").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(
            addrs,
            vec![
                "updates[0].schedule.interval".to_string(),
                "updates[2].schedule.interval".to_string(),
            ]
        );
    }

    #[test]
    fn eval_not_eq_via_any_wildcard() {
        let tree = dependabot_tree();
        let p = MatchPredicate::parse("updates[].schedule.interval != weekly").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(addrs, vec!["updates[1].schedule.interval".to_string()]);
    }

    #[test]
    fn eval_concrete_index_still_works() {
        let tree = dependabot_tree();
        let p = MatchPredicate::parse("updates[1].schedule.interval = monthly").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(addrs, vec!["updates[1].schedule.interval".to_string()]);
    }

    // --- evaluation: presence / absence ---

    #[test]
    fn eval_presence_finds_existing_field() {
        let tree = json!({
            "tool": {"maturin": {"bindings": "pyo3"}},
            "project": {"name": "foo"}
        });
        let p = MatchPredicate::parse("tool.maturin").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert!(addrs.contains(&"tool.maturin".to_string()));
    }

    #[test]
    fn eval_presence_misses_when_absent() {
        let tree = json!({"project": {"name": "foo"}});
        let p = MatchPredicate::parse("tool.maturin").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert!(addrs.is_empty());
    }

    #[test]
    fn eval_presence_expands_through_any() {
        let tree = json!({
            "updates": [
                {"cooldown": {"default-days": 7}},
                {},
                {"cooldown": {"default-days": 14}},
            ]
        });
        let p = MatchPredicate::parse("updates[].cooldown").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert_eq!(
            addrs,
            vec![
                "updates[0].cooldown".to_string(),
                "updates[2].cooldown".to_string(),
            ]
        );
    }

    #[test]
    fn eval_absence_finds_anchor_missing_field() {
        let tree = json!({
            "jobs": {
                "release": {
                    "steps": [
                        {"uses": "actions/checkout@v4"},
                        {"uses": "PyO3/maturin-action@v1", "with": {"command": "upload"}},
                    ]
                }
            }
        });
        let p = MatchPredicate::parse("!name").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        assert!(addrs.contains(&"jobs.release.steps[0]".to_string()));
        assert!(addrs.contains(&"jobs.release.steps[1]".to_string()));
    }

    #[test]
    fn eval_absence_shape_filter() {
        // `!name` should not match array anchors or scalar anchors;
        // only objects can host a `name` key.
        let tree = json!({"items": [1, 2, 3]});
        let p = MatchPredicate::parse("!name").unwrap();
        let addrs = find_match_addresses(&tree, &p);
        // The root object qualifies (no `name` key); the `items` array doesn't.
        assert_eq!(addrs, vec![String::new()]);
    }
}
