//! Pen definitions on disk.
//!
//! Layout:
//! ```text
//! ~/.local/share/nave/pens/
//!   <name>/
//!     pen.toml
//!     repos/
//!       <owner>__<repo>/   (git clone)
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use nave_config::{PenConfig, pen_root};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pen {
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub branch: String,
    pub filter: PenFilter,
    pub repos: Vec<PenRepo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PenFilter {
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PenRepo {
    pub owner: String,
    pub name: String,
    pub default_branch: String,
    pub clone_url: String,
    /// When this repo was last synced against the fleet cache.
    /// Compared against fleet `CacheMeta.last_pushed_at` to detect staleness.
    #[serde(with = "time::serde::rfc3339", default = "default_synced_at")]
    pub synced_at: OffsetDateTime,
}

fn default_synced_at() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH
}

pub fn resolve_pen_root(cfg: &PenConfig) -> Result<PathBuf> {
    if let Some(r) = &cfg.root {
        return Ok(r.clone());
    }
    pen_root()
}

pub fn pen_dir(root: &Path, name: &str) -> PathBuf {
    // The pen's name already starts with "nave/"; use just the suffix as the dir.
    let dir_name = name.strip_prefix("nave/").unwrap_or(name);
    root.join(dir_name)
}

pub fn pen_repos_dir(root: &Path, name: &str) -> PathBuf {
    pen_dir(root, name).join("repos")
}

pub fn pen_toml_path(root: &Path, name: &str) -> PathBuf {
    pen_dir(root, name).join("pen.toml")
}

pub fn pen_repo_clone_dir(root: &Path, name: &str, owner: &str, repo: &str) -> PathBuf {
    pen_repos_dir(root, name).join(format!("{owner}__{repo}"))
}

pub fn write_pen(root: &Path, pen: &Pen) -> Result<()> {
    let dir = pen_dir(root, &pen.name);
    std::fs::create_dir_all(&dir)?;
    let text = toml::to_string_pretty(pen)?;
    let path = pen_toml_path(root, &pen.name);
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load_pen(root: &Path, name: &str) -> Result<Pen> {
    let path = pen_toml_path(root, name);
    if !path.exists() {
        bail!("pen {name} does not exist at {}", path.display());
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).map_err(|e| anyhow!("parsing {}: {e}", path.display()))
}

pub fn list_pens(root: &Path) -> Result<Vec<Pen>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let suffix = entry.file_name().to_string_lossy().into_owned();
        // Pen names stored as "nave/<suffix>"; try both forms when loading.
        let candidate = format!("nave/{suffix}");
        match load_pen(root, &candidate) {
            Ok(p) => out.push(p),
            Err(_) => {
                if let Ok(p) = load_pen(root, &suffix) {
                    out.push(p);
                }
            }
        }
    }
    Ok(out)
}

pub fn remove_pen(root: &Path, name: &str) -> Result<()> {
    let dir = pen_dir(root, name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}
