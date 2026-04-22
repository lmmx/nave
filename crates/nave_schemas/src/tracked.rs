//! Map tracked-path globs to the set of schemas needed to validate them.

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
