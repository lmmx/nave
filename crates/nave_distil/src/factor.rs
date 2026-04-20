//! Post-anti-unification factor discovery.
//!
//! Scans optional-key holes for pairs with zero co-occurrence in the
//! fleet, groups them by greedy transitive closure, and produces
//! per-cohort sub-reports so the user sees tighter templates for
//! repos that share a factor value.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use crate::FileInstance;
use crate::antiunify::{Observations, anti_unify};
use crate::report::{GroupReport, build_group_from_values};

/// A group of optional keys that are pairwise mutually exclusive
/// (zero co-occurrence) across the fleet.
#[derive(Debug, Clone, Serialize)]
pub struct FactorGroup {
    /// Keys (by their hole address) that form this exclusion group.
    pub members: Vec<String>,
    /// For each member: the set of instance indices where it's present.
    pub member_instances: Vec<Vec<usize>>,
    /// Instance indices where *none* of the members are present.
    pub none_cohort: Vec<usize>,
}

/// A cohort is a subset of the fleet plus a sub-report anti-unified
/// over just those instances.
#[derive(Debug, Serialize)]
pub struct Cohort {
    pub label: String,
    pub instance_count: usize,
    pub group_report: GroupReport,
}

#[derive(Debug, Serialize)]
pub struct FactoredReport {
    pub factor: FactorGroup,
    pub cohorts: Vec<Cohort>,
}

/// Build a list of presence-sets for optional-key holes, keyed by the
/// hole's address. Each presence-set is the set of instance indices
/// where the key was present.
pub(crate) fn optional_key_presences(
    observations: &[Observations],
    addresses: &BTreeMap<usize, String>,
    total: usize,
) -> BTreeMap<String, HashSet<usize>> {
    let mut out = BTreeMap::new();
    for (id, obs) in observations.iter().enumerate() {
        // Optional iff fewer instances had the key than the fleet total.
        if obs.instance_indices.len() < total {
            if let Some(addr) = addresses.get(&id) {
                let set: HashSet<usize> = obs.instance_indices.iter().copied().collect();
                out.insert(addr.clone(), set);
            }
        }
    }
    out
}

/// Find zero-co-occurrence groups among the presence-sets.
///
/// Greedy: seed with the pair whose combined presence is largest, then
/// extend by adding any remaining key that has zero intersection with
/// every member already in the group. When no more can be added, start
/// a new group from the remaining keys.
pub(crate) fn find_factors(presences: &BTreeMap<String, HashSet<usize>>) -> Vec<Vec<String>> {
    // Collect into a Vec we can drain from; sort by descending support
    // so seeds prefer well-evidenced keys.
    let mut remaining: Vec<(String, HashSet<usize>)> = presences
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    remaining.sort_by_key(|(_, s)| std::cmp::Reverse(s.len()));

    let mut groups: Vec<Vec<String>> = Vec::new();

    while !remaining.is_empty() {
        // Take the highest-support key as the seed.
        let (seed_key, seed_set) = remaining.remove(0);
        let mut group: Vec<(String, HashSet<usize>)> = vec![(seed_key.clone(), seed_set)];

        // Scan remaining for keys that are pairwise disjoint with
        // everything already in the group.
        let mut i = 0;
        while i < remaining.len() {
            let compatible = group.iter().all(|(_, s)| s.is_disjoint(&remaining[i].1));
            if compatible {
                let (k, s) = remaining.remove(i);
                group.push((k, s));
            } else {
                i += 1;
            }
        }

        // A group of one is useless — it's just "this key is optional,"
        // which the base report already says.
        if group.len() >= 2 {
            groups.push(group.into_iter().map(|(k, _)| k).collect());
        }
    }

    groups
}

/// For a discovered factor, split the fleet into cohorts and anti-unify
/// each cohort's values independently.
pub(crate) fn build_factored_report(
    factor_members: &[String],
    presences: &BTreeMap<String, HashSet<usize>>,
    instances: &[FileInstance],
    all_values: &[Value],
) -> Result<FactoredReport> {
    let total_instances: HashSet<usize> = (0..instances.len()).collect();
    let mut union_of_members: HashSet<usize> = HashSet::new();
    let mut member_instances: Vec<Vec<usize>> = Vec::new();

    for member in factor_members {
        let set = &presences[member];
        union_of_members.extend(set);
        let mut sorted: Vec<usize> = set.iter().copied().collect();
        sorted.sort_unstable();
        member_instances.push(sorted);
    }

    let none_cohort: Vec<usize> = {
        let mut v: Vec<usize> = total_instances
            .difference(&union_of_members)
            .copied()
            .collect();
        v.sort_unstable();
        v
    };

    let mut cohorts: Vec<Cohort> = Vec::new();

    for (member_name, indices) in factor_members.iter().zip(member_instances.iter()) {
        if indices.is_empty() {
            continue;
        }
        let cohort_values: Vec<Value> = indices.iter().map(|&i| all_values[i].clone()).collect();
        let cohort_instances: Vec<FileInstance> =
            indices.iter().map(|&i| instances[i].clone()).collect();
        let report = build_group_from_values("(cohort)", &cohort_instances, &cohort_values)?;
        cohorts.push(Cohort {
            label: member_name.clone(),
            instance_count: indices.len(),
            group_report: report,
        });
    }

    if !none_cohort.is_empty() {
        let cohort_values: Vec<Value> =
            none_cohort.iter().map(|&i| all_values[i].clone()).collect();
        let cohort_instances: Vec<FileInstance> =
            none_cohort.iter().map(|&i| instances[i].clone()).collect();
        let report = build_group_from_values("(cohort)", &cohort_instances, &cohort_values)?;
        cohorts.push(Cohort {
            label: "(none of the above)".to_string(),
            instance_count: none_cohort.len(),
            group_report: report,
        });
    }

    Ok(FactoredReport {
        factor: FactorGroup {
            members: factor_members.to_vec(),
            member_instances,
            none_cohort,
        },
        cohorts,
    })
}

/// Re-anti-unify only the values provided. Used for sub-cohorts.
pub(crate) fn anti_unify_subset(values: &[Value]) -> (crate::antiunify::Template, Vec<Observations>) {
    anti_unify(values)
}

/// Suppress avoidable unused-import warning.
#[allow(dead_code)]
fn _unused(_: BTreeSet<()>) {}
