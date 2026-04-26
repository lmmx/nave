// nave_build/src/fca.rs

//! Formal concept analysis over anti-unification observations.
//!
//! Builds a formal context from the holes produced by [`anti_unify`],
//! enumerates formal concepts via [`odis`], and interprets them as
//! configuration profiles — maximal sets of hole-value bindings shared
//! by a subset of instances.
//!
//! The binarisation follows standard conceptual scaling for nominal
//! attributes: each (hole, value) pair becomes one attribute, and each
//! instance receives a cross for exactly the value it exhibits. Optional
//! holes (`present_in < total`) generate an additional "absent" attribute
//! for the instances that lack the key.

use std::collections::BTreeMap;

use bit_set::BitSet;
use odis::FormalContext;
use odis::algorithms::NextClosure;
use odis::traits::ConceptEnumerator;
use serde::Serialize;
use serde_json::Value;

use crate::antiunify::Observations;
use crate::report::HoleReport;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single binarised attribute in the formal context.
#[derive(Debug, Clone)]
pub(crate) struct Attribute {
    /// Index of the hole in the observations vector.
    pub hole_index: usize,
    /// The address path of the hole (e.g. "on.push.branches").
    pub address: String,
    /// `None` means "key absent"; `Some(v)` means the hole took value `v`.
    pub value: Option<Value>,
}

/// One entry in a profile's intent.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileBinding {
    /// Hole address (e.g. "package-ecosystem").
    pub address: String,
    /// Index into the observations vector.
    pub hole_index: usize,
    /// The value, or `None` for "key absent".
    pub value: Option<Value>,
}

/// A formal concept interpreted in the nave domain: a maximal set of
/// instances sharing a maximal set of hole-value bindings.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    /// Instance indices forming the extent.
    pub instances: Vec<usize>,
    /// Hole-value bindings forming the intent.
    pub bindings: Vec<ProfileBinding>,
    /// Number of instances in the extent.
    pub support: usize,
}

/// Result of running FCA on a single group's observations.
#[derive(Debug, Clone, Serialize)]
pub struct FcaResult {
    /// Non-trivial formal concepts, sorted by descending support.
    pub profiles: Vec<Profile>,
    /// Total number of formal concepts enumerated (before filtering).
    pub total_concepts: usize,
    /// Number of attributes in the formal context.
    pub n_attributes: usize,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Build a formal context from anti-unification observations, enumerate
/// formal concepts, and return interpreted profiles.
///
/// `n_instances` is the total number of instances in the group.
/// `holes` provides the address and optionality metadata for each hole.
/// `observations` are the raw per-hole value vectors from anti-unification.
pub fn analyse(
    observations: &[Observations],
    n_instances: usize,
    holes: &[HoleReport],
) -> FcaResult {
    // Skip FCA if there are fewer than 2 holes (nothing to co-occur).
    if observations.len() < 2 {
        return FcaResult {
            profiles: vec![],
            total_concepts: 0,
            n_attributes: 0,
        };
    }

    let (attributes, ctx) = build_context(observations, n_instances, holes);
    let n_attributes = attributes.len();

    let concepts: Vec<(BitSet, BitSet)> = NextClosure.enumerate_concepts(&ctx).collect();
    let total_concepts = concepts.len();

    let profiles = interpret_concepts(concepts, &attributes, n_instances);

    FcaResult {
        profiles,
        total_concepts,
        n_attributes,
    }
}

// ---------------------------------------------------------------------------
// Context construction (conceptual scaling)
// ---------------------------------------------------------------------------

/// Build an odis `FormalContext` from observations.
///
/// Objects are instances (repos). Attributes are binarised (hole, value)
/// pairs, with an extra "absent" attribute per optional hole.
///
/// Returns `(attribute_metadata, context)`.
fn build_context(
    observations: &[Observations],
    n_instances: usize,
    holes: &[HoleReport],
) -> (Vec<Attribute>, FormalContext<String>) {
    let mut attributes: Vec<Attribute> = Vec::new();

    // For each hole, collect distinct values and build attribute entries.
    // Track (hole_index, serialised_value) → attribute_index.
    let mut value_to_attr: BTreeMap<(usize, String), usize> = BTreeMap::new();
    let mut absent_attr: BTreeMap<usize, usize> = BTreeMap::new();

    // Collect which instances are absent for each optional hole,
    // so we can detect structurally redundant absences.
    let mut absent_sets: Vec<(usize, String, std::collections::HashSet<usize>)> = Vec::new();

    for (hole_idx, (obs, hole)) in observations.iter().zip(holes.iter()).enumerate() {
        let is_optional = hole.present_in < hole.total;

        // Distinct values, keyed by JSON serialisation.
        let mut seen: BTreeMap<String, Value> = BTreeMap::new();
        for v in &obs.values {
            let key = serde_json::to_string(v).unwrap_or_default();
            seen.entry(key).or_insert_with(|| v.clone());
        }

        for (val_key, val) in &seen {
            let attr_idx = attributes.len();
            attributes.push(Attribute {
                hole_index: hole_idx,
                address: hole.address.clone(),
                value: Some(val.clone()),
            });
            value_to_attr.insert((hole_idx, val_key.clone()), attr_idx);
        }

        if is_optional {
            let present: std::collections::HashSet<usize> =
                obs.instance_indices.iter().copied().collect();
            let absent: std::collections::HashSet<usize> =
                (0..n_instances).filter(|i| !present.contains(i)).collect();
            absent_sets.push((hole_idx, hole.address.clone(), absent));
        }
    }

    // Filter out structurally redundant ABSENT attributes: if hole A's
    // address is a prefix of hole B's address and they have the same
    // absent instances, B's absence is implied by A's. Keep only the
    // shallowest (shortest address) for each group of co-absent holes.
    let mut keep_absent: Vec<bool> = vec![true; absent_sets.len()];
    for i in 0..absent_sets.len() {
        if !keep_absent[i] {
            continue;
        }
        for j in 0..absent_sets.len() {
            if i == j || !keep_absent[j] {
                continue;
            }
            let (_, ref addr_i, ref set_i) = absent_sets[i];
            let (_, ref addr_j, ref set_j) = absent_sets[j];
            // If i is a prefix of j and same absent set, j is redundant.
            if addr_j.starts_with(addr_i)
                && (addr_j.len() > addr_i.len())
                && (addr_j.as_bytes().get(addr_i.len()) == Some(&b'.')
                    || addr_j.as_bytes().get(addr_i.len()) == Some(&b'['))
                && set_i == set_j
            {
                keep_absent[j] = false;
            }
        }
    }

    for (idx, keep) in keep_absent.iter().enumerate() {
        if !keep {
            continue;
        }
        let (hole_idx, _, _) = &absent_sets[idx];
        let attr_idx = attributes.len();
        attributes.push(Attribute {
            hole_index: *hole_idx,
            address: holes[*hole_idx].address.clone(),
            value: None,
        });
        absent_attr.insert(*hole_idx, attr_idx);
    }

    // Build per-instance attribute sets.
    let n_attrs = attributes.len();
    let mut instance_attrs: Vec<BitSet> = (0..n_instances)
        .map(|_| BitSet::with_capacity(n_attrs))
        .collect();

    for (hole_idx, obs) in observations.iter().enumerate() {
        let present: std::collections::HashSet<usize> =
            obs.instance_indices.iter().copied().collect();

        for (local, &inst_idx) in obs.instance_indices.iter().enumerate() {
            let val_key = serde_json::to_string(&obs.values[local]).unwrap_or_default();
            if let Some(&attr_idx) = value_to_attr.get(&(hole_idx, val_key)) {
                instance_attrs[inst_idx].insert(attr_idx);
            }
        }

        if let Some(&attr_idx) = absent_attr.get(&hole_idx) {
            for inst_idx in 0..n_instances {
                if !present.contains(&inst_idx) {
                    instance_attrs[inst_idx].insert(attr_idx);
                }
            }
        }
    }

    // Construct the FormalContext.
    let mut ctx = FormalContext::<String>::new();

    for attr in attributes.iter() {
        let label = match &attr.value {
            Some(v) => format!("{}={}", attr.address, short_value(v)),
            None => format!("{}=ABSENT", attr.address),
        };
        ctx.add_attribute(label, &BitSet::new());
    }

    // Add objects with their precomputed attribute sets.
    for (inst_idx, attrs) in instance_attrs.iter().enumerate() {
        ctx.add_object(format!("inst_{inst_idx}"), attrs);
    }

    (attributes, ctx)
}

/// Truncate a JSON value for use in attribute labels.
fn short_value(v: &Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    if s.len() > 40 {
        format!("{}…", &s[..37])
    } else {
        s
    }
}

// ---------------------------------------------------------------------------
// Interpretation
// ---------------------------------------------------------------------------

/// Convert raw concepts into domain-level profiles, filtering out
/// trivial and uninteresting concepts.
fn interpret_concepts(
    concepts: Vec<(BitSet, BitSet)>,
    attributes: &[Attribute],
    n_instances: usize,
) -> Vec<Profile> {
    let mut profiles: Vec<Profile> = Vec::new();

    for (extent, intent) in concepts {
        let support = extent.len();

        // Skip empty extent (bottom).
        if support == 0 {
            continue;
        }
        // Skip full extent with empty intent (top with no shared attributes).
        if support == n_instances && intent.is_empty() {
            continue;
        }

        let instances: Vec<usize> = extent.iter().collect();
        let bindings: Vec<ProfileBinding> = intent
            .iter()
            .map(|attr_idx| {
                let attr = &attributes[attr_idx];
                ProfileBinding {
                    address: attr.address.clone(),
                    hole_index: attr.hole_index,
                    value: attr.value.clone(),
                }
            })
            .collect();

        let distinct_holes: std::collections::HashSet<usize> =
            bindings.iter().map(|b| b.hole_index).collect();
        if distinct_holes.len() < 2 {
            continue;
        }

        profiles.push(Profile {
            instances,
            bindings,
            support,
        });
    }

    profiles.sort_by(|a, b| b.support.cmp(&a.support));
    profiles
}

/// Filter profiles to those where at least one binding's value
/// satisfies at least one of the given match predicates.
pub fn filter_profiles_by_predicates(
    profiles: &[Profile],
    preds: &[nave_config::MatchPredicate],
) -> Vec<Profile> {
    profiles
        .iter()
        .filter(|p| {
            p.bindings.iter().any(|b| match &b.value {
                Some(v) => preds.iter().any(|pred| pred.matches_value(v)),
                None => false,
            })
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::antiunify::Observations;
    use crate::report::{HoleKind, HoleReport, SourceHint};

    fn make_hole(address: &str, present_in: usize, total: usize) -> HoleReport {
        HoleReport {
            address: address.to_string(),
            present_in,
            total,
            distinct_values: vec![],
            kind: HoleKind::String,
            source_hint: SourceHint::Free,
        }
    }

    fn make_obs(indices: Vec<usize>, values: Vec<&str>) -> Observations {
        Observations {
            instance_indices: indices,
            values: values
                .into_iter()
                .map(|s| Value::String(s.to_string()))
                .collect(),
        }
    }

    #[test]
    fn context_construction_shape() {
        // 3 instances, 2 required holes.
        // hole 0: ["a", "a", "b"]  → 2 distinct values → 2 attributes
        // hole 1: ["x", "y", "x"]  → 2 distinct values → 2 attributes
        // Total: 4 attributes, no absent attributes.
        let obs = vec![
            make_obs(vec![0, 1, 2], vec!["a", "a", "b"]),
            make_obs(vec![0, 1, 2], vec!["x", "y", "x"]),
        ];
        let holes = vec![make_hole("h0", 3, 3), make_hole("h1", 3, 3)];
        let (attrs, ctx) = build_context(&obs, 3, &holes);

        assert_eq!(attrs.len(), 4);
        assert_eq!(ctx.objects.len(), 3);
        assert_eq!(ctx.attributes.len(), 4);

        // Instance 0 has h0="a" and h1="x" → its intent should have 2 attributes.
        let inst_0: BitSet = [0].into_iter().collect();
        assert_eq!(ctx.index_intent(&inst_0).len(), 2);
    }

    #[test]
    fn optional_key_produces_absent_attribute() {
        // 4 instances, 1 hole present in 3/4.
        // Instances 0,1,2 have value "v"; instance 3 is absent.
        let obs = vec![make_obs(vec![0, 1, 2], vec!["v", "v", "v"])];
        let holes = vec![make_hole("opt", 3, 4)];
        let (attrs, ctx) = build_context(&obs, 4, &holes);

        // 1 value attribute + 1 absent attribute = 2.
        assert_eq!(attrs.len(), 2);
        assert_eq!(ctx.attributes.len(), 2);

        // Instance 3 should have the absent attribute (index 1).
        let absent_idx = attrs.iter().position(|a| a.value.is_none()).unwrap();
        assert!(ctx.incidence.contains(&(3, absent_idx)));
        assert!(!ctx.incidence.contains(&(0, absent_idx)));
    }

    #[test]
    fn dependabot_finds_co_occurrence() {
        // 9 instances, 2 holes:
        //   hole 0 (package-ecosystem): 8× "github-actions", 1× "cargo"
        //   hole 1 (interval): 6× "weekly", 3× "monthly"
        //
        // Instances 0..5 = github-actions + weekly
        //           6,7  = github-actions + monthly
        //           8    = cargo + monthly
        let obs = vec![
            make_obs(
                (0..9).collect(),
                vec![
                    "github-actions",
                    "github-actions",
                    "github-actions",
                    "github-actions",
                    "github-actions",
                    "github-actions",
                    "github-actions",
                    "github-actions",
                    "cargo",
                ],
            ),
            make_obs(
                (0..9).collect(),
                vec![
                    "weekly", "weekly", "weekly", "weekly", "weekly", "weekly", "monthly",
                    "monthly", "monthly",
                ],
            ),
        ];
        let holes = vec![
            make_hole("package-ecosystem", 9, 9),
            make_hole("schedule.interval", 9, 9),
        ];

        let result = analyse(&obs, 9, &holes);

        // There should be a profile for the 6 github-actions+weekly repos.
        let has_ga_weekly = result.profiles.iter().any(|p| {
            p.support == 6
                && p.bindings.iter().any(|b| {
                    b.address == "package-ecosystem"
                        && b.value == Some(Value::String("github-actions".into()))
                })
                && p.bindings.iter().any(|b| {
                    b.address == "schedule.interval"
                        && b.value == Some(Value::String("weekly".into()))
                })
        });
        assert!(
            has_ga_weekly,
            "expected a profile for 6× github-actions+weekly, got: {:#?}",
            result.profiles
        );
    }

    #[test]
    fn uniform_single_hole_no_profiles() {
        // All 5 instances have same value for one hole → 1 attribute →
        // no concept has bindings from 2+ distinct holes → empty profiles.
        let obs = vec![make_obs(
            vec![0, 1, 2, 3, 4],
            vec!["same", "same", "same", "same", "same"],
        )];
        let holes = vec![make_hole("h0", 5, 5)];

        let result = analyse(&obs, 5, &holes);

        assert!(
            result.profiles.is_empty(),
            "expected no profiles for single uniform hole, got: {:#?}",
            result.profiles
        );
    }
}
