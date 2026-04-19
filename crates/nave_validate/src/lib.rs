//! Walk the fetched checkouts and run a round-trip parse check on each
//! tracked file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use tracing::debug;

use nave_config::cache::{read_repo_meta, read_tracked};
use nave_parse::{Format, RoundTrip, round_trip};

#[derive(Debug, Serialize)]
pub struct FileResult {
    pub owner: String,
    pub repo: String,
    pub path: String,
    pub format: Option<&'static str>,
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ValidationReport {
    pub results: Vec<FileResult>,
    pub totals: Totals,
}

#[derive(Debug, Default, Serialize)]
pub struct Totals {
    pub ok: usize,
    pub drift: usize,
    pub parse_failed: usize,
    pub render_failed: usize,
    pub reparse_failed: usize,
    pub unknown_format: usize,
    pub missing: usize,
}

pub fn run_validate(cache_root: &Path) -> Result<ValidationReport> {
    let mut report = ValidationReport::default();
    let repos_root = cache_root.join("repos");
    if !repos_root.exists() {
        return Ok(report);
    }

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
            validate_repo(cache_root, &owner, &name, &repo_entry.path(), &mut report)?;
        }
    }
    Ok(report)
}

fn validate_repo(
    cache_root: &Path,
    owner: &str,
    name: &str,
    repo_dir: &Path,
    report: &mut ValidationReport,
) -> Result<()> {
    let Some(meta) = read_repo_meta(cache_root, owner, name)? else {
        debug!(%owner, %name, "no repo meta; skipping");
        return Ok(());
    };

    let tracked = read_tracked(cache_root, owner, name)?;
    let checkout = repo_dir.join("checkout");

    for path in tracked.files.keys() {
        let on_disk = checkout.join(path);
        let full = on_disk.clone();

        if !on_disk.exists() {
            report.totals.missing += 1;
            report.results.push(FileResult {
                owner: meta.owner.clone(),
                repo: meta.name.clone(),
                path: path.clone(),
                format: None,
                outcome: "missing",
                detail: Some(
                    "file tracked in cache but not found in checkout; run `nave fetch`".into(),
                ),
            });
            continue;
        }

        let Some(fmt) = Format::from_path(&full) else {
            report.totals.unknown_format += 1;
            report.results.push(FileResult {
                owner: meta.owner.clone(),
                repo: meta.name.clone(),
                path: path.clone(),
                format: None,
                outcome: "unknown_format",
                detail: None,
            });
            continue;
        };

        let bytes = std::fs::read(&full).with_context(|| format!("reading {}", full.display()))?;
        let outcome = round_trip(&bytes, fmt);

        let (label, detail) = split_outcome(&outcome);
        match outcome {
            RoundTrip::Ok => report.totals.ok += 1,
            RoundTrip::SemanticDrift => report.totals.drift += 1,
            RoundTrip::ParseFailed(_) => report.totals.parse_failed += 1,
            RoundTrip::RenderFailed(_) => report.totals.render_failed += 1,
            RoundTrip::ReparseFailed(_) => report.totals.reparse_failed += 1,
        }

        report.results.push(FileResult {
            owner: meta.owner.clone(),
            repo: meta.name.clone(),
            path: path.clone(),
            format: Some(match fmt {
                Format::Toml => "toml",
                Format::Yaml => "yaml",
            }),
            outcome: label,
            detail,
        });
    }

    Ok(())
}

fn split_outcome(o: &RoundTrip) -> (&'static str, Option<String>) {
    match o {
        RoundTrip::Ok => ("ok", None),
        RoundTrip::SemanticDrift => ("drift", None),
        RoundTrip::ParseFailed(e) => ("parse_failed", Some(e.clone())),
        RoundTrip::RenderFailed(e) => ("render_failed", Some(e.clone())),
        RoundTrip::ReparseFailed(e) => ("reparse_failed", Some(e.clone())),
    }
}

/// Convenience: full path of a repo's checkout directory.
pub fn checkout_dir(cache_root: &Path, owner: &str, repo: &str) -> PathBuf {
    cache_root
        .join("repos")
        .join(owner)
        .join(repo)
        .join("checkout")
}
