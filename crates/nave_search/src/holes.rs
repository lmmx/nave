//! Hole-address enrichment for `--output holes`.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use nave_config::{
    NaveConfig,
    address::{Match, walk_matches},
};

#[derive(Debug, Clone, Serialize)]
pub struct HoleHit {
    pub pattern: String,
    pub address: String,
    pub needle: String,
    pub snippet: String,
    pub owner: String,
    pub repo: String,
    pub file_path: String,
}

#[derive(Debug, Clone)]
pub struct MatchedFile {
    pub owner: String,
    pub repo: String,
    pub file_path: String,
    pub pattern: String,
    pub bytes: Vec<u8>,
    pub matched_needle: String,
}

/// Enrich file matches with hole-level addresses.
pub fn enrich_with_holes(
    _cache_root: &Path,
    _cfg: &NaveConfig,
    matches: &[MatchedFile],
) -> Result<Vec<HoleHit>> {
    let mut hits: Vec<HoleHit> = Vec::new();

    for m in matches {
        let Some(tree) = parse_to_json(&m.bytes, &m.file_path) else {
            // Parse failed — surface a whole-file hit so the match isn't lost.
            hits.push(HoleHit {
                pattern: m.pattern.clone(),
                address: "$".to_string(),
                needle: m.matched_needle.clone(),
                snippet: short_snippet(&String::from_utf8_lossy(&m.bytes)),
                owner: m.owner.clone(),
                repo: m.repo.clone(),
                file_path: m.file_path.clone(),
            });
            continue;
        };

        walk_matches(&tree, &m.matched_needle, |addr, what| {
            let snippet = match what {
                Match::Leaf(v) => match v {
                    serde_json::Value::String(s) => short_snippet(s),
                    other => other.to_string(),
                },
                Match::Key(k) => format!("(key) {k}"),
            };
            hits.push(HoleHit {
                pattern: m.pattern.clone(),
                address: addr.to_string(),
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

fn parse_to_json(bytes: &[u8], path: &str) -> Option<serde_json::Value> {
    use nave_parse::{Format, parse_bytes};

    let ext = Path::new(path).extension().and_then(|e| e.to_str());
    let fmt = match ext {
        Some(e) if e.eq_ignore_ascii_case("toml") => Format::Toml,
        Some(e) if e.eq_ignore_ascii_case("yml") || e.eq_ignore_ascii_case("yaml") => Format::Yaml,
        _ => return None,
    };
    let doc = parse_bytes(bytes, fmt).ok()?;
    nave_build::to_common_tree(&doc).ok()
}

fn short_snippet(s: &str) -> String {
    const MAX: usize = 80;
    if s.len() > MAX {
        format!("{}…", &s[..MAX.min(s.len())])
    } else {
        s.to_string()
    }
}
