//! Hole-address enrichment for `--output holes`.
//!
//! Given the per-file matches from `match_repo`, we re-parse each
//! matched file, walk the build template for that file's group, and
//! find the addresses where the matched needle appears in this specific
//! file's contribution. This gives the user "where in the configured
//! template did my match land" without showing data from other repos.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use nave_build::{GroupReport, run_build};
use nave_config::NaveConfig;

#[derive(Debug, Clone, Serialize)]
pub struct HoleHit {
    /// Tracked-path pattern (the build group this address belongs to).
    pub pattern: String,
    /// Address in the template, e.g. `updates[0].package-ecosystem`.
    pub address: String,
    /// The needle that matched.
    pub needle: String,
    /// A short snippet of the value at this address (truncated).
    pub snippet: String,
    /// Which repo contributed this match.
    pub owner: String,
    pub repo: String,
    /// File this address came from.
    pub file_path: String,
}

/// Enrich a set of file matches with hole-level addresses.
///
/// For each matched file, we parse it, look up its build group, and
/// walk the template to find every address where the file's own value
/// at that address contains one of the matched needles.
pub fn enrich_with_holes(
    cache_root: &Path,
    cfg: &NaveConfig,
    matches: &[MatchedFile],
) -> Result<Vec<HoleHit>> {
    // Run build once — we need its templates regardless of which
    // files matched, because the template structure is global.
    let report = run_build(cache_root, cfg, &nave_build::BuildOptions::default())?;

    // Index groups by pattern for fast lookup.
    let groups_by_pattern: BTreeMap<&str, &GroupReport> = report
        .groups
        .iter()
        .map(|g| (g.pattern.as_str(), g))
        .collect();

    let mut hits: Vec<HoleHit> = Vec::new();

    for m in matches {
        let Some(_group) = groups_by_pattern.get(m.pattern.as_str()) else {
            // No template for this pattern (e.g. skipped workflows in an
            // older version, or a pattern with only one instance). We
            // still want to report *something* — surface a single
            // address `$` meaning "the whole file".
            hits.push(HoleHit {
                pattern: m.pattern.clone(),
                address: "$".to_string(),
                needle: m.matched_needle.clone(),
                snippet: short_snippet_bytes(&m.bytes),
                owner: m.owner.clone(),
                repo: m.repo.clone(),
                file_path: m.file_path.clone(),
            });
            continue;
        };

        // Parse this file's value. If parsing fails we can't address
        // into it; fall back to whole-file hit as above.
        let Some(value) = parse_to_json(&m.bytes, &m.file_path) else {
            hits.push(HoleHit {
                pattern: m.pattern.clone(),
                address: "$".to_string(),
                needle: m.matched_needle.clone(),
                snippet: short_snippet_bytes(&m.bytes),
                owner: m.owner.clone(),
                repo: m.repo.clone(),
                file_path: m.file_path.clone(),
            });
            continue;
        };

        // We don't re-walk the template structure directly; instead we
        // walk the file's own value tree with the template's addressing
        // scheme. For each leaf-or-opaque-subtree, if the needle is
        // present in that subtree's serialized form, emit an address.
        //
        // This is simpler and more reliable than trying to align with
        // the template — and it gives finer addresses for literals
        // that happened to contain the needle.
        walk_value(&value, "", &m.matched_needle, &mut |addr, snippet| {
            hits.push(HoleHit {
                pattern: m.pattern.clone(),
                address: addr,
                needle: m.matched_needle.clone(),
                snippet,
                owner: m.owner.clone(),
                repo: m.repo.clone(),
                file_path: m.file_path.clone(),
            });
        });
    }

    Ok(hits)
}

/// Context we need about a single file match for hole enrichment.
#[derive(Debug, Clone)]
pub struct MatchedFile {
    pub owner: String,
    pub repo: String,
    pub file_path: String,
    pub pattern: String,
    pub bytes: Vec<u8>,
    pub matched_needle: String,
}

fn parse_to_json(bytes: &[u8], path: &str) -> Option<Value> {
    use nave_parse::{Format, parse_bytes};
    use std::path::Path;

    let ext = Path::new(path).extension().and_then(|e| e.to_str());

    let fmt = match ext {
        Some(e) if e.eq_ignore_ascii_case("toml") => Format::Toml,
        Some(e) if e.eq_ignore_ascii_case("yml") || e.eq_ignore_ascii_case("yaml") => Format::Yaml,
        _ => return None,
    };

    let doc = parse_bytes(bytes, fmt).ok()?;
    nave_build::to_common_tree(&doc).ok()
}

/// Walk the JSON value tree. For each node:
///   - if it's a string/number/bool leaf, check substring
///   - if it's a scalar leaf that matches, emit the current address
///   - if it's an object/array, recurse
///   - if NO descendant matches but the serialised whole contains the
///     needle (e.g. a key name), emit the containing address instead
///
/// This gives us leaf-precise addresses where possible and graceful
/// fallback when the match is structural (e.g. a key itself).
fn walk_value(value: &Value, path: &str, needle: &str, emit: &mut impl FnMut(String, String)) {
    match value {
        Value::String(s) => {
            if s.contains(needle) {
                emit(path_or_root(path), short_snippet(s));
            }
        }
        Value::Number(n) => {
            let s = n.to_string();
            if s.contains(needle) {
                emit(path_or_root(path), s);
            }
        }
        Value::Bool(b) => {
            let s = b.to_string();
            if s.contains(needle) {
                emit(path_or_root(path), s);
            }
        }
        Value::Null => {}
        Value::Array(items) => {
            // Check if the array's serialised form contains the needle
            // anywhere — if so, recurse; if not, skip entirely (fast path).
            let whole = serde_json::to_string(value).unwrap_or_default();
            if !whole.contains(needle) {
                return;
            }
            for (i, item) in items.iter().enumerate() {
                let sub = format!("{path}[{i}]");
                walk_value(item, &sub, needle, emit);
            }
        }
        Value::Object(map) => {
            // Two possibilities: a key contains the needle, or a value
            // somewhere inside does.
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
                // Key match?
                if k.contains(needle) {
                    emit(sub.clone(), format!("(key) {k}"));
                }
                walk_value(v, &sub, needle, emit);
            }
        }
    }
}

fn path_or_root(p: &str) -> String {
    if p.is_empty() {
        "$".to_string()
    } else {
        p.to_string()
    }
}

fn short_snippet(s: &str) -> String {
    const MAX: usize = 80;
    if s.len() > MAX {
        format!("{}…", &s[..MAX.min(s.len())])
    } else {
        s.to_string()
    }
}

fn short_snippet_bytes(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b);
    short_snippet(&s)
}
