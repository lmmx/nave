//! Map tracked-path globs to the set of schemas needed to validate them.
#![allow(clippy::case_sensitive_file_extension_comparisons)]

use std::collections::BTreeSet;

use crate::id::SchemaId;

/// Given the user's configured `scan.tracked_paths`, return the schemas
/// we'd need cached to validate those files.
pub fn schemas_for_tracked(patterns: &[String]) -> BTreeSet<SchemaId> {
    let mut out = BTreeSet::new();
    for p in patterns {
        let p = p.to_ascii_lowercase();
        if p.contains("dependabot") {
            out.insert(SchemaId::Dependabot);
        }
        if p.contains(".github/workflows") {
            out.insert(SchemaId::GithubWorkflow);
            out.insert(SchemaId::GithubAction);
        }
        if p == "pyproject.toml" {
            out.insert(SchemaId::Pyproject);
        }
    }
    out
}

/// Map a repo-relative path to the schema that governs it, if any.
pub fn schema_for_path(relpath: &str) -> Option<SchemaId> {
    let p = relpath.to_ascii_lowercase();
    if matches!(p.as_str(), "dependabot.yml" | "dependabot.yaml")
        || p.ends_with("/dependabot.yml")
        || p.ends_with("/dependabot.yaml")
    {
        return Some(SchemaId::Dependabot);
    }
    if p.starts_with(".github/workflows/") && (p.ends_with(".yml") || p.ends_with(".yaml")) {
        return Some(SchemaId::GithubWorkflow);
    }
    if matches!(p.as_str(), "action.yml" | "action.yaml")
        || p.ends_with("/action.yml")
        || p.ends_with("/action.yaml")
    {
        return Some(SchemaId::GithubAction);
    }
    if p == "pyproject.toml" || p.ends_with("/pyproject.toml") {
        return Some(SchemaId::Pyproject);
    }
    None
}
