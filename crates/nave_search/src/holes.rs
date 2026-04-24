//! Hole-address enrichment for `--output holes`.
//!
//! A `HoleHit` is a `(pattern, address)` pair with evidence of why
//! that address was flagged — either because a term's needle occurred
//! at (or near) it, or because a `--match` predicate resolved to it.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use nave_config::{
    NaveConfig,
    address::{Match, subtree_at, walk_matches},
};

#[derive(Debug, Clone, Serialize)]
pub struct HoleHit {
    pub pattern: String,
    pub address: String,
    pub evidence: HoleEvidence,
    pub snippet: String,
    pub owner: String,
    pub repo: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HoleEvidence {
    /// The address was produced by walking the tree for a term's
    /// matched needle. `term` is the term's display form (e.g.
    /// `workflow:pytest|nox`); `needle` is the alternative that
    /// actually matched.
    Needle { term: String, needle: String },
    /// The address was emitted directly by a `--match` predicate.
    /// `predicate` is the predicate's raw text.
    Predicate { predicate: String },
}

/// A file that contributed evidence to the search report, with all
/// sources that flagged it. A file may have both needle and predicate
/// sources (term + predicate both matching the same file).
#[derive(Debug, Clone)]
pub struct MatchedFile {
    pub owner: String,
    pub repo: String,
    pub file_path: String,
    pub pattern: String,
    pub bytes: Vec<u8>,
    /// Pairs of `(term_raw, needle)` — for each needle, the tree is
    /// walked to surface every address containing it.
    pub needle_sources: Vec<(String, String)>,
    /// Pairs of `(predicate_raw, concrete_address)` — each address is
    /// surfaced directly, with a snippet taken via `subtree_at`.
    pub predicate_sources: Vec<(String, String)>,
}

/// Enrich file matches with hole-level addresses.
pub fn enrich_with_holes(
    _cache_root: &Path,
    _cfg: &NaveConfig,
    matches: &[MatchedFile],
) -> Result<Vec<HoleHit>> {
    let mut hits: Vec<HoleHit> = Vec::new();

    for m in matches {
        let parsed = parse_to_json(&m.bytes, &m.file_path);

        // Needle-sourced hits: walk the tree for each needle.
        for (term, needle) in &m.needle_sources {
            if let Some(tree) = parsed.as_ref() {
                walk_matches(tree, needle, |addr, what| {
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
                        evidence: HoleEvidence::Needle {
                            term: term.clone(),
                            needle: needle.clone(),
                        },
                        snippet,
                        owner: m.owner.clone(),
                        repo: m.repo.clone(),
                        file_path: m.file_path.clone(),
                    });
                });
            } else {
                // Parse failed — surface a whole-file hit so the match
                // isn't lost.
                hits.push(HoleHit {
                    pattern: m.pattern.clone(),
                    address: "$".to_string(),
                    evidence: HoleEvidence::Needle {
                        term: term.clone(),
                        needle: needle.clone(),
                    },
                    snippet: short_snippet(&String::from_utf8_lossy(&m.bytes)),
                    owner: m.owner.clone(),
                    repo: m.repo.clone(),
                    file_path: m.file_path.clone(),
                });
            }
        }

        // Predicate-sourced hits: the address is already known; pull
        // the snippet from the subtree at that address.
        if let Some(tree) = parsed.as_ref() {
            for (predicate, address) in &m.predicate_sources {
                let snippet = match subtree_at(tree, address) {
                    Some(serde_json::Value::String(s)) => short_snippet(&s),
                    Some(other) => short_snippet(&other.to_string()),
                    None => String::new(),
                };
                hits.push(HoleHit {
                    pattern: m.pattern.clone(),
                    address: address.clone(),
                    evidence: HoleEvidence::Predicate {
                        predicate: predicate.clone(),
                    },
                    snippet,
                    owner: m.owner.clone(),
                    repo: m.repo.clone(),
                    file_path: m.file_path.clone(),
                });
            }
        } else {
            // Parse failed: we can still surface that a predicate hit
            // occurred, but without an address we can trust.
            for (predicate, address) in &m.predicate_sources {
                hits.push(HoleHit {
                    pattern: m.pattern.clone(),
                    address: address.clone(),
                    evidence: HoleEvidence::Predicate {
                        predicate: predicate.clone(),
                    },
                    snippet: String::new(),
                    owner: m.owner.clone(),
                    repo: m.repo.clone(),
                    file_path: m.file_path.clone(),
                });
            }
        }
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
    nave_parse::to_json(&doc).ok()
}

fn short_snippet(s: &str) -> String {
    const MAX: usize = 80;
    if s.len() > MAX {
        format!("{}…", &s[..MAX.min(s.len())])
    } else {
        s.to_string()
    }
}
