use std::collections::BTreeMap;

use serde_json::Value;

/// A template node — the output of anti-unifying a set of `Value`s.
#[derive(Debug, Clone)]
pub enum Template {
    /// All instances had exactly this literal value at this position.
    Literal(Value),

    /// Instances disagreed at this position. The `id` is a stable index
    /// into the observations vector.
    Hole { id: usize },

    /// A JSON object/map. For each key: whether the key is present in
    /// all instances at this scope (required) or some (optional), plus
    /// the anti-unification of its value over the instances where the
    /// key is present.
    Object(BTreeMap<String, Field>),

    /// A JSON array. Positional alignment: same-length arrays zip
    /// element-wise; different lengths fall through to Hole.
    Array(Vec<Template>),

    /// A JSON array with set/bag semantics: elements are matched
    /// across instances by structural similarity (key-set signature)
    /// rather than position. Used for arrays of objects where order
    /// is arbitrary and content is identity — CI workflow steps,
    /// dependency entries, linter rule lists. Each `Field` represents
    /// a cluster of structurally-similar elements across instances,
    /// with `present_in / total` tracking how many instances
    /// contributed a matching element.
    Set(Vec<Field>),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub value: Template,
    /// How many instances at the parent's scope had this key.
    pub present_in: usize,
    /// Total instances at the parent's scope.
    pub total: usize,
}

impl Field {
    pub fn is_required(&self) -> bool {
        self.present_in == self.total
    }
}

/// Anti-unify a set of values into a template, along with the raw
/// observations needed to fill in a report.
pub fn anti_unify(instances: &[Value]) -> (Template, Vec<Observations>) {
    assert!(!instances.is_empty(), "anti_unify called with no instances");
    let mut obs = Vec::new();
    // Top-level scope: every instance index 0..N is in scope.
    let top_scope: Vec<usize> = (0..instances.len()).collect();
    let template = au_rec(instances, &top_scope, &mut obs);
    (template, obs)
}

/// Values observed at a single hole.
///
/// `instance_indices[k]` is the originating instance's index in the
/// top-level list; `values[k]` is the value that instance had at this
/// hole. These two vectors always have the same length. If a hole
/// exists at all, every slot has a value — there is no `Option` here,
/// because optional-key semantics are captured at the `Field` level
/// (via `present_in < total`), and we only recurse into the subset
/// where the key is present.
#[derive(Debug, Clone, Default)]
pub struct Observations {
    pub instance_indices: Vec<usize>,
    pub values: Vec<Value>,
}

/// Anti-unify the subset of `all_instances` given by `scope` (a list of
/// indices into `all_instances`).
fn au_rec(all_instances: &[Value], scope: &[usize], obs: &mut Vec<Observations>) -> Template {
    debug_assert!(!scope.is_empty());

    // Gather the actual values at this scope.
    let vals: Vec<&Value> = scope.iter().map(|&i| &all_instances[i]).collect();

    // All literally equal → Literal.
    if vals.windows(2).all(|w| w[0] == w[1]) {
        return Template::Literal(vals[0].clone());
    }

    // All objects → recurse field-wise.
    if vals.iter().all(|v| matches!(v, Value::Object(_))) {
        return au_object(all_instances, scope, obs);
    }

    // All arrays → decide: set (arrays of objects) or positional (arrays of scalars).
    if vals.iter().all(|v| matches!(v, Value::Array(_))) {
        let arrays: Vec<&Vec<Value>> = vals.iter().map(|v| v.as_array().unwrap()).collect();

        // Auto-detect: if all elements across all arrays are objects, use set semantics.
        let all_elements_are_objects = arrays
            .iter()
            .all(|a| a.iter().all(|elem| matches!(elem, Value::Object(_))));

        if all_elements_are_objects {
            return au_set(all_instances, scope, obs);
        }

        // Otherwise: positional, but only if equal lengths.
        if arrays.iter().all(|a| a.len() == arrays[0].len()) {
            return au_array(all_instances, scope, arrays[0].len(), obs);
        }
    }

    // Mixed types or disagreeing structures → hole, observed at every
    // instance in the current scope.
    let id = obs.len();
    obs.push(Observations {
        instance_indices: scope.to_vec(),
        values: vals.into_iter().cloned().collect(),
    });
    Template::Hole { id }
}

fn au_object(all_instances: &[Value], scope: &[usize], obs: &mut Vec<Observations>) -> Template {
    let total = scope.len();
    let objects: Vec<&serde_json::Map<String, Value>> = scope
        .iter()
        .map(|&i| all_instances[i].as_object().unwrap())
        .collect();

    // Union of keys, preserving first-seen order.
    let mut all_keys: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::default();
    for o in &objects {
        for k in o.keys() {
            if seen.insert(k.clone()) {
                all_keys.push(k.clone());
            }
        }
    }

    let mut fields = BTreeMap::new();
    for key in all_keys {
        // Collect (parent_instance_idx, field_value) pairs for this key.
        let mut sub_parent_indices: Vec<usize> = Vec::new();
        let mut child_vals: Vec<Value> = Vec::new();
        for (i, o) in scope.iter().copied().zip(objects.iter()) {
            if let Some(v) = o.get(&key) {
                sub_parent_indices.push(i);
                child_vals.push(v.clone());
            }
        }
        let present_in = child_vals.len();

        // Recurse on the synthetic child value list with a fresh 0..n
        // scope, then remap any hole observations from local to parent
        // indices. Same pattern as au_array.
        let mark = obs.len();
        let child_scope: Vec<usize> = (0..child_vals.len()).collect();
        let child = au_rec(&child_vals, &child_scope, obs);

        for o in obs.iter_mut().skip(mark) {
            o.instance_indices = o
                .instance_indices
                .iter()
                .map(|&local_idx| sub_parent_indices[local_idx])
                .collect();
        }

        fields.insert(
            key,
            Field {
                value: child,
                present_in,
                total,
            },
        );
    }

    Template::Object(fields)
}

fn au_array(
    all_instances: &[Value],
    scope: &[usize],
    len: usize,
    obs: &mut Vec<Observations>,
) -> Template {
    // For each position i in the array, build a synthetic instance set
    // of the i-th elements. We do this by materialising them and
    // recursing — the index remapping is handled by constructing a
    // fresh "values array" and recursing with a scope of 0..n.
    //
    // However the natural way to preserve attribution is slightly more
    // involved: we want observations to report the *original* instance
    // index, not an index into our synthetic array. So we thread the
    // parent scope through and extract elements on demand.
    let mut elements = Vec::with_capacity(len);
    for i in 0..len {
        // For array element i, every instance in scope contributes its
        // i-th element. The scope of instances stays the same.
        // We need a temporary array-like structure where index j in the
        // scope maps to the i-th element of the j-th in-scope instance.
        //
        // Simplest: build a fresh value list and a synthetic 0..n scope
        // for the child, then remap hole observations back up.
        let child_vals: Vec<Value> = scope
            .iter()
            .map(|&j| all_instances[j].as_array().unwrap()[i].clone())
            .collect();

        let mark = obs.len();
        let child_scope: Vec<usize> = (0..child_vals.len()).collect();
        let child = au_rec(&child_vals, &child_scope, obs);

        // Remap any observations added during this recursion so their
        // instance indices reference the parent scope, not the synthetic
        // 0..n scope of child_vals.
        for o in obs.iter_mut().skip(mark) {
            o.instance_indices = o
                .instance_indices
                .iter()
                .map(|&local_idx| scope[local_idx])
                .collect();
        }

        elements.push(child);
    }
    Template::Array(elements)
}

fn au_set(all_instances: &[Value], scope: &[usize], obs: &mut Vec<Observations>) -> Template {
    let total = scope.len();

    // Compute the key-set intersection across ALL elements in ALL
    // in-scope instances. Only these "core" keys participate in the
    // signature. Optional keys (present in some elements, absent in
    // others) don't affect clustering — they become optional fields
    // within the cluster's anti-unified template.
    let mut core_keys: Option<std::collections::BTreeSet<String>> = None;
    for &global in scope {
        for elem in all_instances[global].as_array().unwrap() {
            if let Some(obj) = elem.as_object() {
                let ks: std::collections::BTreeSet<String> = obj.keys().cloned().collect();
                core_keys = Some(match core_keys {
                    Some(existing) => existing.intersection(&ks).cloned().collect(),
                    None => ks,
                });
            }
        }
    }
    let core_keys = core_keys.unwrap_or_default();

    // Signature is (core key names only, ordinal within this key-group
    // in this instance). Values are NEVER part of the signature — they
    // are what gets anti-unified within each cluster.
    type Signature = (Vec<String>, usize);

    let mut tagged: Vec<(usize, Value, Signature)> = Vec::new();
    for (local, &global) in scope.iter().enumerate() {
        // Count how many times each key-set has appeared in *this* instance.
        let mut ordinals: BTreeMap<Vec<String>, usize> = BTreeMap::new();
        for elem in all_instances[global].as_array().unwrap() {
            let keys = match elem.as_object() {
                Some(obj) => {
                    let mut ks: Vec<String> = obj
                        .keys()
                        .filter(|k| core_keys.contains(k.as_str()))
                        .cloned()
                        .collect();
                    ks.sort();
                    ks
                }
                None => vec!["__scalar__".to_string()],
            };
            let ord = ordinals.entry(keys.clone()).or_insert(0);
            tagged.push((local, elem.clone(), (keys, *ord)));
            *ord += 1;
        }
    }

    // Group by signature.
    let mut clusters: BTreeMap<Signature, Vec<(usize, Value)>> = BTreeMap::new();
    for (local, val, sig) in tagged {
        clusters.entry(sig).or_default().push((local, val));
    }

    let mut fields = Vec::new();
    for (_sig, members) in clusters {
        let parent_indices: Vec<usize> = members.iter().map(|(local, _)| scope[*local]).collect();
        let child_vals: Vec<Value> = members.into_iter().map(|(_, v)| v).collect();

        // present_in counts distinct instances, not occurrences.
        // With the ordinal fix, each instance contributes at most once
        // per cluster, so this dedup is technically redundant — but
        // kept as a safety invariant.
        let present_in = {
            let mut instances: Vec<usize> = parent_indices.clone();
            instances.sort_unstable();
            instances.dedup();
            instances.len()
        };

        let mark = obs.len();
        let child_scope: Vec<usize> = (0..child_vals.len()).collect();
        let child = au_rec(&child_vals, &child_scope, obs);

        // Remap observations to parent scope.
        for o in obs.iter_mut().skip(mark) {
            o.instance_indices = o
                .instance_indices
                .iter()
                .map(|&local_idx| parent_indices[local_idx])
                .collect();
        }

        fields.push(Field {
            value: child,
            present_in,
            total,
        });
    }

    Template::Set(fields)
}
