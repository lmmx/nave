//! Cache layout and on-disk state types.
//!
//! ```text
//! ~/.cache/nave/
//!   meta.toml                           -- CacheMeta
//!   repos/<owner>/<repo>/
//!     meta.toml                         -- RepoMeta
//!     tracked.toml                      -- TrackedFiles
//!     checkout/                         -- (populated by `nave fetch`, out of scope here)
//! ```

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Top-level cache metadata.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheMeta {
    /// The most recent `pushed_at` we've seen across any discovered repo.
    /// Used to build the incremental `pushed:>TS` search query next run.
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_pushed_at: Option<OffsetDateTime>,
    /// When we last ran discovery (informational).
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_discovery_at: Option<OffsetDateTime>,
    /// `"gh"` | `"token_env"` | `"anonymous"`
    pub auth_mode: Option<String>,
    /// Username last used for discovery.
    pub username: Option<String>,
}

/// Per-repo metadata.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoMeta {
    pub owner: String,
    pub name: String,
    pub default_branch: String,
    pub clone_url: String,
    pub tree_sha: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub pushed_at: Option<OffsetDateTime>,
}

/// Mapping of repo-root-relative path → blob sha, for files we're actively tracking.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TrackedFiles {
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

pub fn meta_path(cache_root: &Path) -> PathBuf {
    cache_root.join("meta.toml")
}

pub fn repo_dir(cache_root: &Path, owner: &str, repo: &str) -> PathBuf {
    cache_root.join("repos").join(owner).join(repo)
}

pub fn read_cache_meta(cache_root: &Path) -> Result<CacheMeta> {
    let path = meta_path(cache_root);
    if !path.exists() {
        return Ok(CacheMeta::default());
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    Ok(toml::from_str(&text)?)
}

pub fn write_cache_meta(cache_root: &Path, meta: &CacheMeta) -> Result<()> {
    std::fs::create_dir_all(cache_root)?;
    let text = toml::to_string_pretty(meta)?;
    atomic_write(&meta_path(cache_root), &text)
}

pub fn read_repo_meta(cache_root: &Path, owner: &str, repo: &str) -> Result<Option<RepoMeta>> {
    let path = repo_dir(cache_root, owner, repo).join("meta.toml");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(Some(toml::from_str(&text)?))
}

pub fn write_repo_meta(cache_root: &Path, meta: &RepoMeta) -> Result<()> {
    let dir = repo_dir(cache_root, &meta.owner, &meta.name);
    std::fs::create_dir_all(&dir)?;
    let text = toml::to_string_pretty(meta)?;
    atomic_write(&dir.join("meta.toml"), &text)
}

pub fn read_tracked(cache_root: &Path, owner: &str, repo: &str) -> Result<TrackedFiles> {
    let path = repo_dir(cache_root, owner, repo).join("tracked.toml");
    if !path.exists() {
        return Ok(TrackedFiles::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&text)?)
}

pub fn write_tracked(
    cache_root: &Path,
    owner: &str,
    repo: &str,
    files: &TrackedFiles,
) -> Result<()> {
    let dir = repo_dir(cache_root, owner, repo);
    std::fs::create_dir_all(&dir)?;
    let text = toml::to_string_pretty(files)?;
    atomic_write(&dir.join("tracked.toml"), &text)
}

/// Write-then-rename to avoid torn files if we crash mid-write.
fn atomic_write(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
