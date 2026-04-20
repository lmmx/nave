//! Predicate-based search over the nave cache.

pub mod holes;

use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use time::OffsetDateTime;
use tracing::debug;

use nave_config::{
    NaveConfig, PathMatcher, Term,
    cache::{RepoMeta, read_repo_meta, read_tracked},
};

pub use holes::HoleHit;

#[derive(Debug, Clone, Serialize)]
pub struct SearchReport {
    /// One entry per matching repo.
    pub repos: Vec<RepoMatch>,
    /// Number of repos considered (had a checkout on disk).
    pub repos_considered: usize,
    /// Number of repos skipped because no checkout existed.
    pub repos_without_checkout: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<HoleHit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoMatch {
    pub owner: String,
    pub repo: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub pushed_at: Option<OffsetDateTime>,
    /// Per-term evidence — one `TermHit` per term, listing which files
    /// satisfied the term for this repo.
    pub hits: Vec<TermHit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TermHit {
    /// Original term text, e.g. `workflow:pytest|nox`.
    pub term: String,
    /// Files that satisfied this term, each annotated with the needle
    /// that actually matched.
    pub files: Vec<FileMatch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileMatch {
    pub path: String,
    pub matched_needle: String,
}

pub struct SearchOptions {
    pub terms: Vec<Term>,
    pub ignore_case: bool,
    /// Whether to enrich results with hole-level addresses.
    pub enrich_holes: bool,
}

pub fn run_search(
    cache_root: &Path,
    cfg: &NaveConfig,
    options: &SearchOptions,
) -> Result<SearchReport> {
    let repos_root = cache_root.join("repos");
    let mut report = SearchReport {
        repos: Vec::new(),
        repos_considered: 0,
        repos_without_checkout: 0,
        holes: Vec::new(),
    };

    if !repos_root.exists() {
        return Ok(report);
    }

    // Pre-build matchers for each tracked-path pattern — so we can
    // classify a file's path into its pattern once, rather than testing
    // every term against every pattern for every file.
    let pattern_matchers: Vec<(String, PathMatcher)> = cfg
        .scan
        .tracked_paths
        .iter()
        .map(|pat| {
            let m = PathMatcher::new(std::slice::from_ref(pat), cfg.scan.case_insensitive)?;
            Ok::<_, anyhow::Error>((pat.clone(), m))
        })
        .collect::<Result<Vec<_>>>()?;

    for owner_entry in std::fs::read_dir(&repos_root)? {
        let owner_entry = owner_entry?;
        if !owner_entry.file_type()?.is_dir() {
            continue;
        }
        let owner = owner_entry.file_name().to_string_lossy().into_owned();

        for repo_entry in std::fs::read_dir(owner_entry.path())? {
            let repo_entry = repo_entry?;
            if !repo_entry.file_type()?.is_dir() {
                continue;
            }
            let name = repo_entry.file_name().to_string_lossy().into_owned();

            let Some(meta) = read_repo_meta(cache_root, &owner, &name)? else {
                continue;
            };
            let checkout = repo_entry.path().join("checkout");
            if !checkout.exists() {
                report.repos_without_checkout += 1;
                continue;
            }
            report.repos_considered += 1;

            if let Some(matched) =
                match_repo(cache_root, &meta, &checkout, &pattern_matchers, options)?
            {
                report.repos.push(matched);
            }
        }
    }

    if options.enrich_holes && !report.repos.is_empty() {
        let matched_files = collect_matched_files(&report, cache_root, cfg);
        report.holes = holes::enrich_with_holes(cache_root, cfg, &matched_files)?;
    }

    Ok(report)
}

fn match_repo(
    cache_root: &Path,
    meta: &RepoMeta,
    checkout: &Path,
    pattern_matchers: &[(String, PathMatcher)],
    options: &SearchOptions,
) -> Result<Option<RepoMatch>> {
    let tracked = read_tracked(cache_root, &meta.owner, &meta.name)?;
    if tracked.files.is_empty() {
        return Ok(None);
    }

    // Classify each tracked file into its matching tracked-path pattern.
    // We load bytes once per file and reuse across terms.
    let mut files: Vec<(String, String, Vec<u8>)> = Vec::new();
    //                ^pattern ^path   ^bytes
    for path in tracked.files.keys() {
        let Some(pattern) = classify(pattern_matchers, path) else {
            continue;
        };
        let on_disk = checkout.join(path);
        let Ok(bytes) = std::fs::read(&on_disk) else {
            debug!(owner = %meta.owner, repo = %meta.name, %path, "could not read file");
            continue;
        };
        files.push((pattern.to_string(), path.clone(), bytes));
    }

    // Evaluate each term. A term is satisfied iff at least one
    // in-scope file's content matches one of the term's needles.
    let mut hits: Vec<TermHit> = Vec::new();
    for term in &options.terms {
        let mut file_matches: Vec<FileMatch> = Vec::new();
        for (pattern, path, bytes) in &files {
            if !term.applies_to_pattern(pattern) {
                continue;
            }
            if let Some(needle) = term.matches_content(bytes, options.ignore_case) {
                file_matches.push(FileMatch {
                    path: path.clone(),
                    matched_needle: needle.to_string(),
                });
            }
        }
        if file_matches.is_empty() {
            return Ok(None); // this term failed → repo doesn't match
        }
        hits.push(TermHit {
            term: term.raw.clone(),
            files: file_matches,
        });
    }

    Ok(Some(RepoMatch {
        owner: meta.owner.clone(),
        repo: meta.name.clone(),
        pushed_at: meta.pushed_at,
        hits,
    }))
}

fn classify<'a>(pattern_matchers: &'a [(String, PathMatcher)], path: &str) -> Option<&'a str> {
    pattern_matchers
        .iter()
        .find(|(_, m)| m.is_match(path))
        .map(|(p, _)| p.as_str())
}

fn collect_matched_files(
    report: &SearchReport,
    cache_root: &Path,
    cfg: &NaveConfig,
) -> Vec<holes::MatchedFile> {
    use std::collections::BTreeSet;

    let mut out: Vec<holes::MatchedFile> = Vec::new();
    let mut seen: BTreeSet<(String, String, String, String)> = BTreeSet::new();

    for r in &report.repos {
        let checkout = cache_root
            .join("repos")
            .join(&r.owner)
            .join(&r.repo)
            .join("checkout");
        for hit in &r.hits {
            for fm in &hit.files {
                let key = (
                    r.owner.clone(),
                    r.repo.clone(),
                    fm.path.clone(),
                    fm.matched_needle.clone(),
                );
                if !seen.insert(key) {
                    continue;
                }
                let Ok(bytes) = std::fs::read(checkout.join(&fm.path)) else {
                    continue;
                };
                let pattern =
                    pattern_for_path(cfg, &fm.path).unwrap_or_else(|| "(unknown)".to_string());
                out.push(holes::MatchedFile {
                    owner: r.owner.clone(),
                    repo: r.repo.clone(),
                    file_path: fm.path.clone(),
                    pattern,
                    bytes,
                    matched_needle: fm.matched_needle.clone(),
                });
            }
        }
    }
    out
}

fn pattern_for_path(cfg: &NaveConfig, path: &str) -> Option<String> {
    for pat in &cfg.scan.tracked_paths {
        let m = PathMatcher::new(std::slice::from_ref(pat), cfg.scan.case_insensitive).ok()?;
        if m.is_match(path) {
            return Some(pat.clone());
        }
    }
    None
}
