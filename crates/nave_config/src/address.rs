//! Address machinery for structural match filtering.
//!
//! Addresses use dotted path notation with bracketed array indices,
//! matching what `nave build` emits in hole reports, e.g.:
//!   jobs.release.steps[1].with.command

use serde_json::Value;

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
    walk_with_emit(value, String::new(), needle, &mut emit);
}

/// Find every address in `value` where `needle` appears.
pub fn find_addresses(value: &Value, needle: &str) -> Vec<String> {
    let mut out = Vec::new();
    walk_matches(value, needle, |addr, _| out.push(addr.to_string()));
    out
}

fn walk_with_emit(
    value: &Value,
    path: String,
    needle: &str,
    emit: &mut impl FnMut(&str, Match<'_>),
) {
    match value {
        Value::String(s) => {
            if s.contains(needle) {
                emit(path_or_root(&path), Match::Leaf(value));
            }
        }
        Value::Number(n) => {
            if n.to_string().contains(needle) {
                emit(path_or_root(&path), Match::Leaf(value));
            }
        }
        Value::Bool(b) => {
            if b.to_string().contains(needle) {
                emit(path_or_root(&path), Match::Leaf(value));
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
                walk_with_emit(item, sub, needle, emit);
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
                walk_with_emit(v, sub, needle, emit);
            }
        }
    }
}

fn path_or_root(p: &str) -> &str {
    if p.is_empty() { "$" } else { p }
}

/// Parse a dotted/bracketed address into segments.
enum Segment<'a> {
    Key(&'a str),
    Index(usize),
}

fn parse_address(addr: &str) -> Vec<Segment<'_>> {
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
                let end = addr[i..]
                    .find(']')
                    .map(|j| i + j + 1)
                    .unwrap_or(bytes.len());
                let inner = &addr[i + 1..end - 1];
                if let Ok(n) = inner.parse::<usize>() {
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
    out
}

/// Walk the ancestor chain of `addr` through `root`, yielding each
/// ancestor address that corresponds to an object node. The root is
/// represented as the empty string; the leaf itself is included iff
/// it resolves to an object.
pub fn object_ancestors(root: &Value, addr: &str) -> Vec<String> {
    let segments = parse_address(addr);
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
                current_path.push_str(&format!("[{i}]"));
                cursor = next;
                if cursor.is_object() {
                    out.push(current_path.clone());
                }
            }
        }
    }

    out
}

/// Deepest non-root object ancestor shared by `a` and `b` in `root`,
/// or `None` if they share only the document root or nothing.
pub fn deepest_shared_object_ancestor(root: &Value, a: &str, b: &str) -> Option<String> {
    let anc_a = object_ancestors(root, a);
    let anc_b = object_ancestors(root, b);
    let mut lca: Option<String> = None;
    for (x, y) in anc_a.iter().zip(anc_b.iter()) {
        if x == y {
            lca = Some(x.clone());
        } else {
            break;
        }
    }
    lca.filter(|s| !s.is_empty())
}

/// Resolve an address to a subtree in `root`. Returns `None` if the
/// address doesn't exist.
pub fn subtree_at(root: &Value, addr: &str) -> Option<Value> {
    if addr.is_empty() {
        return Some(root.clone());
    }
    let segments = parse_address(addr);
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
        }
    }
    Some(cursor.clone())
}

/// Enumerate every address in `value` that resolves to an object node,
/// including the root (as the empty string). Used by match predicates
/// to consider every object as a candidate anchor.
pub fn find_addresses_all_objects(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_objects(value, String::new(), &mut out);
    out
}

fn walk_objects(value: &Value, path: String, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            out.push(path.clone());
            for (k, v) in map {
                let sub = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk_objects(v, sub, out);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                let sub = format!("{path}[{i}]");
                walk_objects(item, sub, out);
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
    fn lca_is_step_for_uses_and_with_command() {
        let tree = json!({
            "jobs": {
                "release": {
                    "steps": [
                        {
                            "uses": "PyO3/maturin-action@v1",
                            "with": {"command": "upload"}
                        }
                    ]
                }
            }
        });
        assert_eq!(
            deepest_shared_object_ancestor(
                &tree,
                "jobs.release.steps[0].uses",
                "jobs.release.steps[0].with.command",
            ),
            Some("jobs.release.steps[0]".to_string()),
        );
    }

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
    fn only_document_root_in_common() {
        let tree = json!({
            "a": "maturin-action@v1",
            "b": {"command": "upload"}
        });
        assert!(deepest_shared_object_ancestor(&tree, "a", "b.command").is_none());
    }
}
