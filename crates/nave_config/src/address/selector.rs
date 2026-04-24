#![allow(clippy::doc_link_with_quotes)]
//! Selectors: wildcard address patterns.
//!
//! A selector is a sequence of segments where each segment is either a
//! literal key, a literal index, or `[]` — which matches any child of
//! the current node (object value or array element alike).
//!
//! Grammar:
//!
//!   selector := leading? (segment)*
//!   leading  := "."            ; optional leading dot for ergonomics
//!   segment  := "." key        ; object key
//!             | "[" index "]"  ; literal array index
//!             | "[]"           ; any child (object values or array elements)
//!
//! Examples:
//!
//!   jobs.release.steps[0]     one concrete address
//!   jobs.[].steps.[]          every step across every job
//!   [].updates.[].interval    every interval in every dependabot update
//!                              across every top-level item
//!
//! We follow jq's uniform `.[]` rather than distinguishing `.*` from
//! `[*]`: config trees mix objects and arrays freely and forcing users
//! to pick the right one at each level is needless friction.

use anyhow::{Result, anyhow};
use serde_json::Value;

/// A single segment of a selector pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorSegment {
    /// Literal object key.
    Key(String),
    /// Literal array index.
    Index(usize),
    /// Match any child of the current node — object values or array
    /// elements alike.
    Any,
}

/// Parse a selector string into a sequence of segments.
///
/// Accepts a leading `.` for ergonomics (`.jobs.[].steps` and
/// `jobs.[].steps` are equivalent).
pub fn parse_selector(input: &str) -> Result<Vec<SelectorSegment>> {
    let input = input.strip_prefix('.').unwrap_or(input);
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0usize;
    let mut key_start = 0usize;

    let flush_key = |out: &mut Vec<SelectorSegment>, s: &str| {
        if !s.is_empty() {
            out.push(SelectorSegment::Key(s.to_string()));
        }
    };

    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                flush_key(&mut out, &input[key_start..i]);
                i += 1;
                key_start = i;
            }
            b'[' => {
                flush_key(&mut out, &input[key_start..i]);
                let close = input[i..]
                    .find(']')
                    .ok_or_else(|| anyhow!("unclosed `[` in selector: {input:?}"))?;
                let inner = &input[i + 1..i + close];
                if inner.is_empty() {
                    out.push(SelectorSegment::Any);
                } else {
                    let idx: usize = inner.parse().map_err(|_| {
                        anyhow!("non-numeric array index {inner:?} in selector: {input:?}")
                    })?;
                    out.push(SelectorSegment::Index(idx));
                }
                i += close + 1;
                key_start = i;
            }
            _ => i += 1,
        }
    }
    flush_key(&mut out, &input[key_start..]);

    Ok(out)
}

/// Resolve a selector against `root`, returning every concrete address
/// that matches.
pub fn resolve_selector(root: &Value, selector: &[SelectorSegment]) -> Vec<String> {
    let mut out = Vec::new();
    walk_selector(root, selector, 0, String::new(), &mut out);
    out
}

fn walk_selector(
    cursor: &Value,
    selector: &[SelectorSegment],
    depth: usize,
    path: String,
    out: &mut Vec<String>,
) {
    if depth == selector.len() {
        out.push(path);
        return;
    }
    match &selector[depth] {
        SelectorSegment::Key(k) => {
            let Value::Object(map) = cursor else { return };
            let Some(next) = map.get(k) else { return };
            let sub = if path.is_empty() {
                k.clone()
            } else {
                format!("{path}.{k}")
            };
            walk_selector(next, selector, depth + 1, sub, out);
        }
        SelectorSegment::Index(idx) => {
            let Value::Array(items) = cursor else { return };
            let Some(next) = items.get(*idx) else { return };
            let sub = format!("{path}[{idx}]");
            walk_selector(next, selector, depth + 1, sub, out);
        }
        SelectorSegment::Any => match cursor {
            Value::Object(map) => {
                for (k, v) in map {
                    let sub = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    walk_selector(v, selector, depth + 1, sub, out);
                }
            }
            Value::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    let sub = format!("{path}[{i}]");
                    walk_selector(v, selector, depth + 1, sub, out);
                }
            }
            _ => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_plain_path() {
        let s = parse_selector("jobs.release.steps[0]").unwrap();
        assert_eq!(
            s,
            vec![
                SelectorSegment::Key("jobs".into()),
                SelectorSegment::Key("release".into()),
                SelectorSegment::Key("steps".into()),
                SelectorSegment::Index(0),
            ]
        );
    }

    #[test]
    fn parse_leading_dot_is_optional() {
        let a = parse_selector(".jobs.steps").unwrap();
        let b = parse_selector("jobs.steps").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn parse_any_bracket() {
        let s = parse_selector("jobs.[].steps.[]").unwrap();
        assert_eq!(
            s,
            vec![
                SelectorSegment::Key("jobs".into()),
                SelectorSegment::Any,
                SelectorSegment::Key("steps".into()),
                SelectorSegment::Any,
            ]
        );
    }

    #[test]
    fn parse_any_without_leading_dot() {
        let s = parse_selector("jobs[]").unwrap();
        assert_eq!(
            s,
            vec![SelectorSegment::Key("jobs".into()), SelectorSegment::Any]
        );
    }

    #[test]
    fn parse_unclosed_bracket_errors() {
        let err = parse_selector("jobs[").unwrap_err();
        assert!(format!("{err}").contains("unclosed"));
    }

    #[test]
    fn parse_non_numeric_index_errors() {
        let err = parse_selector("jobs[abc]").unwrap_err();
        assert!(format!("{err}").contains("non-numeric"));
    }

    fn workflow() -> Value {
        json!({
            "jobs": {
                "build": {
                    "steps": [
                        {"uses": "actions/checkout@v4"},
                        {"uses": "actions/setup-python@v5"},
                    ]
                },
                "release": {
                    "steps": [
                        {"uses": "actions/checkout@v4"},
                        {"uses": "PyO3/maturin-action@v1"},
                    ]
                }
            }
        })
    }

    #[test]
    fn resolve_concrete_path() {
        let tree = workflow();
        let sel = parse_selector("jobs.release.steps[1]").unwrap();
        assert_eq!(
            resolve_selector(&tree, &sel),
            vec!["jobs.release.steps[1]".to_string()]
        );
    }

    #[test]
    fn resolve_any_over_object() {
        let tree = workflow();
        let sel = parse_selector("jobs.[]").unwrap();
        let mut got = resolve_selector(&tree, &sel);
        got.sort();
        assert_eq!(
            got,
            vec!["jobs.build".to_string(), "jobs.release".to_string()]
        );
    }

    #[test]
    fn resolve_any_over_array() {
        let tree = workflow();
        let sel = parse_selector("jobs.release.steps[]").unwrap();
        let mut got = resolve_selector(&tree, &sel);
        got.sort();
        assert_eq!(
            got,
            vec![
                "jobs.release.steps[0]".to_string(),
                "jobs.release.steps[1]".to_string(),
            ]
        );
    }

    #[test]
    fn resolve_mixed_any_across_objects_and_arrays() {
        let tree = workflow();
        let sel = parse_selector("jobs.[].steps.[]").unwrap();
        let mut got = resolve_selector(&tree, &sel);
        got.sort();
        assert_eq!(
            got,
            vec![
                "jobs.build.steps[0]".to_string(),
                "jobs.build.steps[1]".to_string(),
                "jobs.release.steps[0]".to_string(),
                "jobs.release.steps[1]".to_string(),
            ]
        );
    }

    #[test]
    fn resolve_missing_key_yields_empty() {
        let tree = workflow();
        let sel = parse_selector("jobs.nope.steps[]").unwrap();
        assert!(resolve_selector(&tree, &sel).is_empty());
    }

    #[test]
    fn resolve_out_of_bounds_index_yields_empty() {
        let tree = workflow();
        let sel = parse_selector("jobs.release.steps[99]").unwrap();
        assert!(resolve_selector(&tree, &sel).is_empty());
    }

    #[test]
    fn resolve_any_on_scalar_is_empty() {
        let tree = json!({"x": 1});
        let sel = parse_selector("x.[]").unwrap();
        assert!(resolve_selector(&tree, &sel).is_empty());
    }
}
