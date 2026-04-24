#![allow(clippy::doc_link_with_quotes)]
//! Address machinery for structural match filtering.
//!
//! Addresses use dotted path notation with bracketed array indices,
//! matching what `nave build` emits in hole reports, e.g.:
//!   jobs.release.steps[1].with.command
//!
//! Addresses also accept `[]` as a wildcard over array elements, giving
//! one-to-many resolution. E.g. `updates[].schedule.interval` picks out
//! the `interval` of every element of `updates`.

mod selector;
pub use selector::{SelectorSegment, parse_selector, resolve_selector};

use anyhow::{Result, anyhow};
use serde_json::Value;
use std::fmt::Write;

/// What was matched at a given address, so callers can format snippets.
pub enum Match<'a> {
    /// The match was a substring of a scalar leaf's value.
    Leaf(&'a Value),
    /// The match was an object key; the string is the key itself.
    Key(&'a str),
}

/// Walk `value` and invoke `emit` for every address where `needle`
/// appears — as a substring of a string/number/bool leaf, or as an
/// object key.
pub fn walk_matches(value: &Value, needle: &str, mut emit: impl FnMut(&str, Match<'_>)) {
    walk_with_emit(value, "", needle, &mut emit);
}

/// Find every address in `value` where `needle` appears.
pub fn find_addresses(value: &Value, needle: &str) -> Vec<String> {
    let mut out = Vec::new();
    walk_matches(value, needle, |addr, _| out.push(addr.to_string()));
    out
}

fn walk_with_emit(value: &Value, path: &str, needle: &str, emit: &mut impl FnMut(&str, Match<'_>)) {
    match value {
        Value::String(s) => {
            if s.contains(needle) {
                emit(path_or_root(path), Match::Leaf(value));
            }
        }
        Value::Number(n) => {
            if n.to_string().contains(needle) {
                emit(path_or_root(path), Match::Leaf(value));
            }
        }
        Value::Bool(b) => {
            if b.to_string().contains(needle) {
                emit(path_or_root(path), Match::Leaf(value));
            }
        }
        Value::Null => {}
        Value::Array(items) => {
            let whole = serde_json::to_string(value).unwrap_or_default();
            if !whole.contains(needle) {
                return;
            }
            for (i, item) in items.iter().enumerate() {
                let sub = format!("{path}[{i}]");
                walk_with_emit(item, &sub, needle, emit);
            }
        }
        Value::Object(map) => {
            let whole = serde_json::to_string(value).unwrap_or_default();
            if !whole.contains(needle) {
                return;
            }
            for (k, v) in map {
                let sub = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if k.contains(needle) {
                    emit(&sub, Match::Key(k));
                }
                walk_with_emit(v, &sub, needle, emit);
            }
        }
    }
}

fn path_or_root(p: &str) -> &str {
    if p.is_empty() { "$" } else { p }
}

/// Parse a dotted/bracketed address into segments.
///
/// Grammar:
///   address  := (segment)*
///   segment  := "." key        ; object key
///             | "[" index "]"  ; literal array index
///             | "[]"           ; any array element (wildcard)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment<'a> {
    Key(&'a str),
    Index(usize),
    Any,
}

/// Parse an address string into segments.
///
/// Returns an error for malformed brackets (unclosed `[`, non-numeric
/// non-empty bracket contents). Previously this was silently lenient,
/// which made bad queries return zero results instead of complaining.
pub fn parse_address(addr: &str) -> Result<Vec<Segment<'_>>> {
    let mut out = Vec::new();
    let bytes = addr.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                if i > start {
                    out.push(Segment::Key(&addr[start..i]));
                }
                i += 1;
                start = i;
            }
            b'[' => {
                if i > start {
                    out.push(Segment::Key(&addr[start..i]));
                }
                let rel = addr[i..]
                    .find(']')
                    .ok_or_else(|| anyhow!("unclosed `[` in address: {addr:?}"))?;
                let end = i + rel + 1;
                let inner = &addr[i + 1..end - 1];
                if inner.is_empty() {
                    out.push(Segment::Any);
                } else {
                    let n: usize = inner.parse().map_err(|_| {
                        anyhow!("non-numeric array index {inner:?} in address: {addr:?}")
                    })?;
                    out.push(Segment::Index(n));
                }
                i = end;
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        out.push(Segment::Key(&addr[start..]));
    }
    Ok(out)
}

/// Walk the ancestor chain of `addr` through `root`, yielding each
/// ancestor address that corresponds to an object node. The root is
/// represented as the empty string; the leaf itself is included iff
/// it resolves to an object.
///
/// Addresses passed here are expected to be *concrete* — produced by
/// `resolve_rel_path` or `find_addresses`, not user-supplied wildcards.
/// A malformed address logs and returns an empty list; this function
/// is not a user-validation point.
pub fn object_ancestors(root: &Value, addr: &str) -> Vec<String> {
    let Ok(segments) = parse_address(addr) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    let mut cursor: &Value = root;
    let mut current_path = String::new();

    if cursor.is_object() {
        out.push(String::new());
    }

    for seg in &segments {
        match seg {
            Segment::Key(k) => {
                let Value::Object(map) = cursor else {
                    break;
                };
                let Some(next) = map.get(*k) else { break };
                if !current_path.is_empty() {
                    current_path.push('.');
                }
                current_path.push_str(k);
                cursor = next;
                if cursor.is_object() {
                    out.push(current_path.clone());
                }
            }
            Segment::Index(i) => {
                let Value::Array(items) = cursor else { break };
                let Some(next) = items.get(*i) else { break };
                let _ = write!(current_path, "[{i}]");
                cursor = next;
                if cursor.is_object() {
                    out.push(current_path.clone());
                }
            }
            Segment::Any => {
                // `object_ancestors` is called on concrete addresses
                // produced by the resolver; `Any` should not appear.
                break;
            }
        }
    }

    out
}

/// Resolve a concrete address to a subtree in `root`. Returns `None`
/// if the address doesn't exist or contains a wildcard (use
/// `resolve_rel_path` for wildcard-bearing paths).
pub fn subtree_at(root: &Value, addr: &str) -> Option<Value> {
    if addr.is_empty() {
        return Some(root.clone());
    }
    let segments = parse_address(addr).ok()?;
    let mut cursor = root;
    for seg in &segments {
        match seg {
            Segment::Key(k) => {
                let Value::Object(map) = cursor else {
                    return None;
                };
                cursor = map.get(*k)?;
            }
            Segment::Index(i) => {
                let Value::Array(items) = cursor else {
                    return None;
                };
                cursor = items.get(*i)?;
            }
            Segment::Any => return None,
        }
    }
    Some(cursor.clone())
}

/// Resolve a path (possibly containing `[]` wildcards) against `root`,
/// returning every `(concrete_address, value)` pair the path yields.
///
/// The returned addresses are concrete — `[]` in the input is replaced
/// with the actual index `[i]` in each result — so callers can use
/// them directly with `subtree_at`, `object_ancestors`, etc.
///
/// An empty path returns `[("", root)]`.
pub fn resolve_rel_path(root: &Value, path: &str) -> Result<Vec<(String, Value)>> {
    let segments = parse_address(path)?;
    let mut out = Vec::new();
    walk_rel(root, &segments, 0, String::new(), &mut out);
    Ok(out)
}

fn walk_rel(
    cursor: &Value,
    segments: &[Segment<'_>],
    depth: usize,
    path: String,
    out: &mut Vec<(String, Value)>,
) {
    if depth == segments.len() {
        out.push((path, cursor.clone()));
        return;
    }
    match &segments[depth] {
        Segment::Key(k) => {
            let Value::Object(map) = cursor else { return };
            let Some(next) = map.get(*k) else { return };
            let sub = if path.is_empty() {
                (*k).to_string()
            } else {
                format!("{path}.{k}")
            };
            walk_rel(next, segments, depth + 1, sub, out);
        }
        Segment::Index(i) => {
            let Value::Array(items) = cursor else { return };
            let Some(next) = items.get(*i) else { return };
            let sub = format!("{path}[{i}]");
            walk_rel(next, segments, depth + 1, sub, out);
        }
        Segment::Any => {
            let Value::Array(items) = cursor else {
                // `[]` only iterates arrays. If the user wants "any
                // object value", they can either use a selector (in
                // `nave_config::address::selector`) or name the key
                // explicitly. This restriction keeps address semantics
                // predictable for YAML/TOML schema-shaped trees where
                // arrays and objects mean different things.
                return;
            };
            for (i, v) in items.iter().enumerate() {
                let sub = format!("{path}[{i}]");
                walk_rel(v, segments, depth + 1, sub, out);
            }
        }
    }
}

/// Enumerate every address in `value` that resolves to an object node,
/// including the root (as the empty string). Used by match predicates
/// to consider every object as a candidate anchor.
pub fn find_addresses_all_objects(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_objects(value, "", &mut out);
    out
}

fn walk_objects(value: &Value, path: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            out.push(path.to_string());
            for (k, v) in map {
                let sub = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk_objects(v, &sub, out);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                let sub = format!("{path}[{i}]");
                walk_objects(item, &sub, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subtree_at_extracts_object() {
        let tree = json!({
            "jobs": {
                "release": {
                    "steps": [{"uses": "x", "with": {"command": "upload"}}]
                }
            }
        });
        let sub = subtree_at(&tree, "jobs.release.steps[0]").unwrap();
        assert_eq!(sub, json!({"uses": "x", "with": {"command": "upload"}}));
    }

    #[test]
    fn parse_address_rejects_unclosed_bracket() {
        let err = parse_address("foo[").unwrap_err();
        assert!(format!("{err}").contains("unclosed"));
    }

    #[test]
    fn parse_address_rejects_non_numeric_index() {
        let err = parse_address("foo[abc]").unwrap_err();
        assert!(format!("{err}").contains("non-numeric"));
    }

    #[test]
    fn parse_address_accepts_any_bracket() {
        let segs = parse_address("updates[].schedule.interval").unwrap();
        assert_eq!(
            segs,
            vec![
                Segment::Key("updates"),
                Segment::Any,
                Segment::Key("schedule"),
                Segment::Key("interval"),
            ]
        );
    }

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
    fn resolve_rel_path_expands_any_over_array() {
        let tree = dependabot_tree();
        let hits = resolve_rel_path(&tree, "updates[].schedule.interval").unwrap();
        let addrs: Vec<&str> = hits.iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(
            addrs,
            vec![
                "updates[0].schedule.interval",
                "updates[1].schedule.interval",
                "updates[2].schedule.interval",
            ]
        );
        let vals: Vec<&str> = hits.iter().map(|(_, v)| v.as_str().unwrap_or("")).collect();
        assert_eq!(vals, vec!["weekly", "monthly", "weekly"]);
    }

    #[test]
    fn resolve_rel_path_empty_returns_root() {
        let tree = dependabot_tree();
        let hits = resolve_rel_path(&tree, "").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, "");
    }

    #[test]
    fn resolve_rel_path_on_scalar_is_empty() {
        let tree = json!({"x": 1});
        let hits = resolve_rel_path(&tree, "x[]").unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn resolve_rel_path_concrete_still_works() {
        let tree = dependabot_tree();
        let hits = resolve_rel_path(&tree, "updates[1].schedule.interval").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, "updates[1].schedule.interval");
        assert_eq!(hits[0].1.as_str(), Some("monthly"));
    }
}
