//! Anti-unification over tracked config files.
//!
//! Groups files by the glob pattern that matched them, anti-unifies each
//! group into a template with holes, and reports observed value
//! distributions per hole.
//!
//! When `--co-occur` is set and multiple `--where` terms are given,
//! instances are co-occurrence sites (subtrees) rather than whole files.

mod antiunify;
mod report;
mod value;

pub use antiunify::{Template, anti_unify};
pub use report::{BuildReport, GroupReport, HoleReport, InstanceRef, SourceHint};
pub use value::to_common_tree;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;
use tracing::{debug, warn};

use nave_config::{
    NaveConfig, PathMatcher, Term,
    address::{find_addresses, subtree_at},
    cache::{read_repo_meta, read_tracked},
    match_pred::{MatchPredicate, find_match_addresses},
};
use nave_parse::parse_file;

#[derive(Debug, Default)]
pub struct BuildOptions {
    /// Only include files satisfying every term. Empty = include all.
    pub where_terms: Vec<Term>,
    /// Structural predicate of the form `[scope:]path op literal`, where
    /// `op` is `=` (exact) or `~` (substring). Matches tree nodes whose
    /// relative `path` resolves to a scalar satisfying the comparison.
    /// Composes with `--where` and `--co-occur`.
    pub match_preds: Vec<MatchPredicate>,
    /// If true, anti-unify subtrees at co-occurrence sites rather than
    /// whole files. A co-occurrence site is the deepest non-root object
    /// ancestor shared by an anchor-term match and at least one match
    /// from each other term. Only meaningful with ≥ 2 `where_terms`.
    pub co_occur: bool,
}

/// Walk the cache and produce a build report.
#[allow(clippy::too_many_lines)]
pub fn run_build(
    cache_root: &Path,
    cfg: &NaveConfig,
    options: &BuildOptions,
) -> Result<BuildReport> {
    let repos_root = cache_root.join("repos");
    let mut report = BuildReport::default();

    if !repos_root.exists() {
        return Ok(report);
    }

    let mut groups: BTreeMap<String, Vec<FileInstance>> = BTreeMap::new();

    let per_pattern: Vec<(String, PathMatcher)> = cfg
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

                // Scope check first — cheap and independent of parsing.
                if !options.where_terms.is_empty()
                    && !options
                        .where_terms
                        .iter()
                        .all(|t| t.applies_to_pattern(pattern))
                {
                    continue;
                }

                // Parse once.
                let doc = match parse_file(&on_disk) {
                    Ok(d) => d,
                    Err(e) => {
                        warn!(%owner, %name, %path, "parse failed: {e}");
                        continue;
                    }
                };
                let full_tree = match to_common_tree(&doc) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(%owner, %name, %path, "tree conversion failed: {e}");
                        continue;
                    }
                };

                let no_filter = options.where_terms.is_empty() && options.match_preds.is_empty();

                // Scope check applies to both --where and --match when the file
                // would otherwise be considered.
                if !no_filter {
                    let scope_ok_where = options
                        .where_terms
                        .iter()
                        .all(|t| t.applies_to_pattern(pattern));
                    let scope_ok_match = options
                        .match_preds
                        .iter()
                        .all(|p| p.applies_to_pattern(pattern));
                    if !scope_ok_where || !scope_ok_match {
                        continue;
                    }
                }

                let instances: Vec<FileInstance> = if no_filter {
                    vec![FileInstance {
                        owner: owner.clone(),
                        repo: name.clone(),
                        path: path.clone(),
                        site_address: None,
                        value: full_tree,
                    }]
                } else if !options.co_occur {
                    // Document-wide: every term + every predicate must have ≥1 hit.
                    let where_ok = options.where_terms.iter().all(|t| {
                        t.needles
                            .iter()
                            .any(|needle| !find_addresses(&full_tree, needle).is_empty())
                    });
                    let match_ok = options
                        .match_preds
                        .iter()
                        .all(|p| !find_match_addresses(&full_tree, p).is_empty());
                    if !(where_ok && match_ok) {
                        continue;
                    }
                    vec![FileInstance {
                        owner: owner.clone(),
                        repo: name.clone(),
                        path: path.clone(),
                        site_address: None,
                        value: full_tree,
                    }]
                } else {
                    // --co-occur: a site qualifies iff every constraint has ≥1 hit inside it.
                    // Candidates are object-ancestors of any hit; we keep the deepest
                    // (minimal under containment) qualifying set. No anchor — constraints
                    // are treated symmetrically.
                    let mut addrs_per_constraint: Vec<Vec<String>> =
                        Vec::with_capacity(options.where_terms.len() + options.match_preds.len());

                    let mut any_empty = false;
                    for t in &options.where_terms {
                        let mut addrs: Vec<String> = Vec::new();
                        for needle in &t.needles {
                            addrs.extend(find_addresses(&full_tree, needle));
                        }
                        if addrs.is_empty() {
                            any_empty = true;
                            break;
                        }
                        addrs_per_constraint.push(addrs);
                    }
                    if !any_empty {
                        for p in &options.match_preds {
                            let addrs = find_match_addresses(&full_tree, p);
                            if addrs.is_empty() {
                                any_empty = true;
                                break;
                            }
                            addrs_per_constraint.push(addrs);
                        }
                    }
                    if any_empty || addrs_per_constraint.is_empty() {
                        continue;
                    }

                    // Flatten (constraint_index, address) for the qualification check.
                    let num_constraints = addrs_per_constraint.len();
                    let all_hits: Vec<(usize, &str)> = addrs_per_constraint
                        .iter()
                        .enumerate()
                        .flat_map(|(ci, addrs)| addrs.iter().map(move |a| (ci, a.as_str())))
                        .collect();

                    // Candidate sites: object-ancestors of every hit, deduped.
                    let mut candidates: Vec<String> = all_hits
                        .iter()
                        .flat_map(|(_, a)| nave_config::address::object_ancestors(&full_tree, a))
                        .filter(|anc| !anc.is_empty())
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect();

                    // Qualify: every constraint has ≥1 hit inside.
                    candidates.retain(|cand| {
                        (0..num_constraints)
                            .all(|ci| all_hits.iter().any(|(c, a)| *c == ci && is_within(cand, a)))
                    });

                    // Deepest-only: drop candidates strictly containing another qualifier.
                    let sites: Vec<String> = candidates
                        .iter()
                        .filter(|cand| {
                            !candidates
                                .iter()
                                .any(|other| other != *cand && is_within(cand, other))
                        })
                        .cloned()
                        .collect();

                    if sites.is_empty() {
                        continue;
                    }

                    sites
                        .into_iter()
                        .filter_map(|site_addr| {
                            subtree_at(&full_tree, &site_addr).map(|subtree| FileInstance {
                                owner: owner.clone(),
                                repo: name.clone(),
                                path: path.clone(),
                                site_address: Some(site_addr),
                                value: subtree,
                            })
                        })
                        .collect()
                };

                if instances.is_empty() {
                    continue;
                }

                for inst in instances {
                    groups.entry(pattern.to_string()).or_default().push(inst);
                }
            }
        }
    }

    for (pattern, instances) in groups {
        if instances.is_empty() {
            continue;
        }
        let group = report::build_group(&pattern, &instances);
        report.groups.push(group);
    }

    Ok(report)
}

/// Is `addr` within (or equal to) the subtree rooted at `root_addr`?
fn is_within(root_addr: &str, addr: &str) -> bool {
    if root_addr.is_empty() {
        return true;
    }
    addr == root_addr
        || addr.starts_with(&format!("{root_addr}."))
        || addr.starts_with(&format!("{root_addr}["))
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

#[derive(Debug)]
pub(crate) struct FileInstance {
    pub owner: String,
    pub repo: String,
    pub path: String,
    /// `None` means "the whole file"; `Some(addr)` means this instance
    /// is the subtree rooted at `addr` within the file.
    pub site_address: Option<String>,
    pub value: Value,
}
