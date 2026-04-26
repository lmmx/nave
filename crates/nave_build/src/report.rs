use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;
use serde_json::Value;

use crate::{FileInstance, FcaResult};
use crate::antiunify::{Observations, Template, anti_unify};

#[derive(Debug, Default, Serialize)]
pub struct BuildReport {
    pub groups: Vec<GroupReport>,
    pub skipped: Vec<(String, usize, String)>,
}

#[derive(Debug, Serialize)]
pub struct GroupReport {
    pub pattern: String,
    pub instance_count: usize,
    pub instances: Vec<InstanceRef>,
    pub template_text: String,
    pub holes: Vec<HoleReport>,
    pub fca: FcaResult,
}

#[derive(Debug, Serialize)]
pub struct InstanceRef {
    pub owner: String,
    pub repo: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_address: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HoleReport {
    pub address: String,
    pub present_in: usize,
    pub total: usize,
    pub distinct_values: Vec<(Value, usize)>,
    pub kind: HoleKind,
    pub source_hint: SourceHint,
}

#[derive(Debug, Serialize, Clone)]
pub enum HoleKind {
    Bool,
    Integer,
    Number,
    String,
    Array,
    Object,
    Mixed,
    OptionalKey,
}

#[derive(Debug, Serialize, Clone)]
pub enum SourceHint {
    Free,
    DerivedFromRepoName,
    ConstantWhenPresent,
}

pub(crate) fn build_group(pattern: &str, instances: &[FileInstance]) -> GroupReport {
    let values: Vec<Value> = instances.iter().map(|inst| inst.value.clone()).collect();

    let (template, observations) = anti_unify(&values);

    let mut hole_addresses: BTreeMap<usize, String> = BTreeMap::new();
    collect_addresses(&template, String::new(), &mut hole_addresses);

    let total = instances.len();
    let repo_names: Vec<String> = instances.iter().map(|i| i.repo.clone()).collect();

    let mut holes = Vec::with_capacity(observations.len());
    for (id, obs) in observations.iter().enumerate() {
        let address = hole_addresses
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("?{id}"));
        holes.push(summarise_hole(address, obs, total, &repo_names));
    }
    holes.sort_by(|a, b| a.address.cmp(&b.address));

    eprintln!(
        "fca::analyse: n_instances={}, n_observations={}, holes={}",
        total,
        observations.len(),
        holes.len()
    );
    let fca_result = crate::fca::analyse(&observations, total, &holes);
    eprintln!("fca::analyse complete");

    let template_text = render_template(&template, 0);

    GroupReport {
        pattern: pattern.to_string(),
        instance_count: total,
        instances: instances
            .iter()
            .map(|i| InstanceRef {
                owner: i.owner.clone(),
                repo: i.repo.clone(),
                path: i.path.clone(),
                site_address: i.site_address.clone(),
            })
            .collect(),
        template_text,
        holes,
        fca: fca_result,
    }
}

fn summarise_hole(
    address: String,
    obs: &Observations,
    total: usize,
    repo_names: &[String],
) -> HoleReport {
    let present_in = obs.values.len();

    let mut tally: BTreeMap<String, (Value, usize)> = BTreeMap::new();
    for val in &obs.values {
        let key = serde_json::to_string(val).unwrap_or_default();
        tally
            .entry(key)
            .and_modify(|e| e.1 += 1)
            .or_insert_with(|| (val.clone(), 1));
    }

    let mut distinct_values: Vec<(Value, usize)> = tally.into_values().collect();
    distinct_values.sort_by_key(|b| std::cmp::Reverse(b.1));

    let kind = classify_hole(&obs.values, present_in, total);
    let source_hint = detect_source(obs, repo_names);

    HoleReport {
        address,
        present_in,
        total,
        distinct_values,
        kind,
        source_hint,
    }
}

fn classify_hole(values: &[Value], present_in: usize, total: usize) -> HoleKind {
    if present_in < total {
        return HoleKind::OptionalKey;
    }
    let mut kinds: std::collections::HashSet<&'static str> = std::collections::HashSet::default();
    for v in values {
        kinds.insert(match v {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Number(n) if n.is_i64() || n.is_u64() => "int",
            Value::Number(_) => "num",
            Value::String(_) => "str",
            Value::Array(_) => "arr",
            Value::Object(_) => "obj",
        });
    }
    if kinds.len() > 1 {
        return HoleKind::Mixed;
    }
    match kinds.into_iter().next().unwrap_or("str") {
        "bool" => HoleKind::Bool,
        "int" => HoleKind::Integer,
        "num" => HoleKind::Number,
        "arr" => HoleKind::Array,
        "obj" => HoleKind::Object,
        _ => HoleKind::String,
    }
}

fn detect_source(obs: &Observations, repo_names: &[String]) -> SourceHint {
    if obs.values.len() >= 2 {
        let subset_repos: Vec<&str> = obs
            .instance_indices
            .iter()
            .map(|&i| repo_names[i].as_str())
            .collect();
        let all_match = obs
            .values
            .iter()
            .zip(subset_repos.iter())
            .all(|(v, &repo)| match v.as_str() {
                Some(s) => s == repo || pep503_eq(s, repo),
                None => false,
            });
        let distinct: std::collections::HashSet<&str> = subset_repos.iter().copied().collect();
        if all_match && distinct.len() >= 2 {
            return SourceHint::DerivedFromRepoName;
        }
    }

    if obs.values.len() >= 2 && obs.values.windows(2).all(|w| w[0] == w[1]) {
        return SourceHint::ConstantWhenPresent;
    }

    SourceHint::Free
}

fn pep503_eq(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.to_ascii_lowercase().replace('_', "-")
    }
    norm(a) == norm(b)
}

fn collect_addresses(t: &Template, path: String, out: &mut BTreeMap<usize, String>) {
    match t {
        Template::Literal(_) => {}
        Template::Hole { id } => {
            out.insert(
                *id,
                if path.is_empty() {
                    "$".to_string()
                } else {
                    path
                },
            );
        }
        Template::Object(fields) => {
            for (key, field) in fields {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect_addresses(&field.value, next, out);
            }
        }
        Template::Array(elems) => {
            for (i, elem) in elems.iter().enumerate() {
                let next = format!("{path}[{i}]");
                collect_addresses(elem, next, out);
            }
        }
    }
}

fn render_template(t: &Template, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    match t {
        Template::Literal(v) => render_literal(v),
        Template::Hole { id } => format!("⟨?{id}⟩"),
        Template::Object(fields) => {
            let mut s = String::new();
            for (i, (key, field)) in fields.iter().enumerate() {
                if i > 0 || indent > 0 {
                    s.push('\n');
                }
                let optional_marker = if field.is_required() { "" } else { "?" };
                s.push_str(&pad);
                s.push_str(key);
                s.push_str(optional_marker);
                s.push_str(": ");
                match &field.value {
                    Template::Literal(v) => s.push_str(&render_literal(v)),
                    Template::Hole { id } => write!(s, "⟨?{id}⟩").unwrap(),
                    nested @ (Template::Object(_) | Template::Array(_)) => {
                        s.push_str(&render_template(nested, indent + 1));
                    }
                }
            }
            s
        }
        Template::Array(elems) => {
            let mut s = String::new();
            for elem in elems {
                s.push('\n');
                s.push_str(&pad);
                s.push_str("- ");
                match elem {
                    Template::Literal(v) => s.push_str(&render_literal(v)),
                    Template::Hole { id } => write!(s, "⟨?{id}⟩").unwrap(),
                    nested @ (Template::Object(_) | Template::Array(_)) => {
                        let rendered = render_template(nested, indent + 1);
                        s.push_str(rendered.trim_start());
                    }
                }
            }
            s
        }
    }
}

fn render_literal(v: &Value) -> String {
    match v {
        Value::String(s) => format!("\"{s}\""),
        other => other.to_string(),
    }
}
