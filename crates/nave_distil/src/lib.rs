//! Anti-unification over tracked config files.
//!
//! Groups files by the glob pattern that matched them, anti-unifies each
//! group into a template with holes, and reports observed value
//! distributions per hole.

mod antiunify;
mod factor;
mod report;
mod value;

pub use antiunify::{Template, anti_unify};
pub use factor::{Cohort, FactorGroup, FactoredReport};
pub use report::{DistilReport, GroupReport, HoleReport, SourceHint};
pub use value::to_common_tree;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use tracing::{debug, warn};

use nave_config::{
    NaveConfig, PathMatcher,
    cache::{read_repo_meta, read_tracked},
};
use nave_parse::{Document, parse_file};

/// Walk the cache and produce a distillation report.
pub fn run_distil(cache_root: &Path, cfg: &NaveConfig) -> Result<DistilReport> {
    let repos_root = cache_root.join("repos");
    let mut report = DistilReport::default();

    if !repos_root.exists() {
        return Ok(report);
    }

    // Group files across repos by which glob pattern matched them.
    // Key: the original tracked_paths pattern string.
    // Value: list of (repo_ident, path_in_repo, parsed_document).
    let mut groups: BTreeMap<String, Vec<FileInstance>> = BTreeMap::new();

    // Build per-pattern matchers so we can attribute each file to exactly
    // one pattern. A file matching multiple patterns picks the first one
    // in config order — mirrors how humans read the list.
    let per_pattern: Vec<(String, PathMatcher)> = cfg
        .discovery
        .tracked_paths
        .iter()
        .map(|pat| {
            let m = PathMatcher::new(std::slice::from_ref(pat), cfg.discovery.case_insensitive)?;
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

            let Some(_meta) = read_repo_meta(cache_root, &owner, &name)? else {
                continue;
            };
            let tracked = read_tracked(cache_root, &owner, &name)?;
            let checkout = repo_entry.path().join("checkout");

            for path in tracked.files.keys() {
                let Some(pattern) = first_matching(&per_pattern, path) else {
                    continue;
                };
                let on_disk = checkout.join(path);
                if !on_disk.exists() {
                    debug!(%owner, %name, %path, "tracked but missing on disk");
                    continue;
                }
                match parse_file(&on_disk) {
                    Ok(doc) => {
                        groups
                            .entry(pattern.to_string())
                            .or_default()
                            .push(FileInstance {
                                owner: owner.clone(),
                                repo: name.clone(),
                                path: path.clone(),
                                doc,
                            });
                    }
                    Err(e) => {
                        warn!(%owner, %name, %path, "parse failed: {e}");
                    }
                }
            }
        }
    }

    for (pattern, instances) in groups {
        if instances.is_empty() {
            continue;
        }
        if pattern.starts_with(".github/workflows/") {
            report.skipped.push((
                pattern,
                instances.len(),
                "workflow YAML — positional alignment inadequate; not yet supported".into(),
            ));
            continue;
        }
        let mut group = report::build_group(&pattern, &instances)?;
        augment_with_factors(&mut group, &instances)?;
        report.groups.push(group);
    }

    Ok(report)
}

fn augment_with_factors(group: &mut report::GroupReport, instances: &[FileInstance]) -> Result<()> {
    // Re-derive the things factor detection needs. This is wasted work
    // relative to a deeper integration but keeps the factor module
    // self-contained.

    let values: Vec<serde_json::Value> = instances
        .iter()
        .map(to_common_tree_safe)
        .collect::<Result<Vec<_>>>()?;

    let (_template, observations) = crate::antiunify::anti_unify(&values);

    // Rebuild the address map, the same way build_group_from_values did.
    let (template_for_addr, _) = crate::antiunify::anti_unify(&values);
    let mut addresses: std::collections::BTreeMap<usize, String> =
        std::collections::BTreeMap::new();
    crate::report::collect_addresses(&template_for_addr, String::new(), &mut addresses);

    let presences =
        crate::factor::optional_key_presences(&observations, &addresses, instances.len());
    if presences.is_empty() {
        return Ok(());
    }

    let factor_groups = crate::factor::find_factors(&presences);
    for members in factor_groups {
        let factored =
            crate::factor::build_factored_report(&members, &presences, instances, &values);
        group.factors.push(factored);
    }
    Ok(())
}

fn to_common_tree_safe(inst: &FileInstance) -> Result<serde_json::Value> {
    crate::value::to_common_tree(&inst.doc)
}

fn first_matching<'a>(per_pattern: &'a [(String, PathMatcher)], path: &str) -> Option<&'a str> {
    per_pattern.iter().find_map(|(pat, m)| {
        if m.is_match(path) {
            Some(pat.as_str())
        } else {
            None
        }
    })
}

#[derive(Debug, Clone)]
pub(crate) struct FileInstance {
    pub owner: String,
    pub repo: String,
    pub path: String,
    pub doc: Document,
}
