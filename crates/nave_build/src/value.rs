use anyhow::{Context, Result};
use serde_json::Value;

use nave_parse::Document;

/// Convert a parsed `Document` into the common `serde_json::Value` tree.
///
/// Loses format-specific details (TOML datetimes become strings, YAML
/// anchors are already expanded at parse time, etc.). Those are not
/// meaningful for structural comparison, which is all we need here.
pub fn to_common_tree(doc: &Document) -> Result<Value> {
    match doc {
        Document::Toml(v) => serde_json::to_value(v).context("toml → json conversion"),
        Document::Yaml(v) => serde_json::to_value(v).context("yaml → json conversion"),
    }
}
