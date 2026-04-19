use serde::Deserialize;
use time::OffsetDateTime;

#[derive(Debug, Clone, Deserialize)]
pub struct Repo {
    pub name: String,
    pub full_name: String,
    pub default_branch: String,
    pub clone_url: String,
    pub fork: bool,
    pub archived: bool,
    #[serde(with = "time::serde::rfc3339::option", default)]
    pub pushed_at: Option<OffsetDateTime>,
    pub owner: RepoOwner,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoOwner {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TreeResponse {
    pub sha: String,
    pub tree: Vec<TreeEntry>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub sha: String,
}

/// Response shape for `GET /search/repositories`.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchResponse {
    pub total_count: u64,
    pub items: Vec<Repo>,
}
