use std::collections::BTreeMap;

use serde_json::Value;

/// A template node — the output of anti-unifying a set of `Value`s.
#[derive(Debug, Clone)]
pub enum Template {
    /// All instances had exactly this literal value at this position.
    Literal(Value),

    /// Instances disagreed at this position. The `id` is a stable index
    /// into the report's hole table; observed values and their per-
    /// instance attribution are tracked there, not here.
    Hole { id: usize },

    /// A JSON object/map. For each key: whether the key is present in
    /// all instances (required) or some (optional), plus the anti-
    /// unification of its value over the instances where it's present.
    Object(BTreeMap<String, Field>),

    /// A JSON array. For the first pass we do positional alignment:
    /// same-length arrays zip element-wise; different lengths fall
    /// through to Hole.
    Array(Vec<Template>),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub value: Template,
    /// How many instances had this key.
    pub present_in: usize,
    /// Total instance count for the parent object.
    pub total: usize,
}

impl Field {
    pub fn is_required(&self) -> bool {
        self.present_in == self.total
    }
}

/// Anti-unify a set of values into a template, along with the raw
/// observations needed to fill in a report.
///
/// Returns `(template, observations)` where `observations[hole_id]` is
/// the per-instance value slice (one entry per input instance).
pub fn anti_unify(instances: &[Value]) -> (Template, Vec<Observations>) {
    assert!(!instances.is_empty(), "anti_unify called with no instances");
    let mut obs = Vec::new();
    let template = au_rec(instances, &mut obs);
    (template, obs)
}

/// Values observed at a single hole, one slot per originating instance.
#[derive(Debug, Clone, Default)]
pub struct Observations {
    /// Per-instance value at this hole. `None` means the hole's parent
    /// key was absent from this instance.
    pub per_instance: Vec<Option<Value>>,
}

fn au_rec(instances: &[Value], obs: &mut Vec<Observations>) -> Template {
    // All literally equal → Literal.
    if instances.windows(2).all(|w| w[0] == w[1]) {
        return Template::Literal(instances[0].clone());
    }

    // All objects → recurse field-wise.
    if instances.iter().all(|v| matches!(v, Value::Object(_))) {
        return au_object(instances, obs);
    }

    // All arrays of equal length → recurse positionally.
    if instances.iter().all(|v| matches!(v, Value::Array(_))) {
        let arrays: Vec<&Vec<Value>> = instances.iter().map(|v| v.as_array().unwrap()).collect();
        if arrays.iter().map(|a| a.len()).all(|n| n == arrays[0].len()) {
            return au_array(&arrays, obs);
        }
        // Differing lengths → hole (whole array treated as opaque).
    }

    // Mixed types or disagreeing structures → hole.
    let id = obs.len();
    obs.push(Observations {
        per_instance: instances.iter().map(|v| Some(v.clone())).collect(),
    });
    Template::Hole { id }
}

fn au_object(instances: &[Value], obs: &mut Vec<Observations>) -> Template {
    let total = instances.len();
    let objects: Vec<&serde_json::Map<String, Value>> =
        instances.iter().map(|v| v.as_object().unwrap()).collect();

    // Union of all keys, preserving first-seen order where possible.
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
        let present: Vec<&Value> = objects.iter().filter_map(|o| o.get(&key)).collect();
        let present_in = present.len();

        if present_in == total {
            // Required key. Recurse normally.
            let present_owned: Vec<Value> = present.into_iter().cloned().collect();
            let child = au_rec(&present_owned, obs);
            fields.insert(
                key,
                Field {
                    value: child,
                    present_in,
                    total,
                },
            );
        } else {
            // Optional key. The value (where present) becomes a hole
            // whose Observations align with the *parent*'s instance
            // order, with None for absent instances.
            let id = obs.len();
            let per_instance: Vec<Option<Value>> =
                objects.iter().map(|o| o.get(&key).cloned()).collect();
            obs.push(Observations { per_instance });
            fields.insert(
                key,
                Field {
                    value: Template::Hole { id },
                    present_in,
                    total,
                },
            );
        }
    }

    Template::Object(fields)
}

fn au_array(arrays: &[&Vec<Value>], obs: &mut Vec<Observations>) -> Template {
    let len = arrays[0].len();
    let mut elements = Vec::with_capacity(len);
    for i in 0..len {
        let slice: Vec<Value> = arrays.iter().map(|a| a[i].clone()).collect();
        elements.push(au_rec(&slice, obs));
    }
    Template::Array(elements)
}
