//! Configuration layering for nave.
//!
//! Precedence (lowest → highest):
//!   1. Baked-in defaults (`NaveConfig::default`)
//!   2. User config at `~/.config/nave.toml`
//!   3. Environment variables prefixed `NAVE_`
//!   4. CLI overrides supplied by the binary

pub mod cache;
pub mod paths;

use std::path::PathBuf;

use figment2::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

pub use crate::paths::{cache_root, user_config_path};

/// The fully-resolved nave configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NaveConfig {
    pub github: GithubConfig,
    pub cache: CacheConfig,
    pub discovery: DiscoveryConfig,
}

impl Default for NaveConfig {
    fn default() -> Self {
        Self {
            github: GithubConfig::default(),
            cache: CacheConfig::default(),
            discovery: DiscoveryConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GithubConfig {
    /// GitHub username. `None` means "ask" or "probe via gh CLI".
    pub username: Option<String>,
    /// Whether to try `gh auth status` / `gh auth token` to fill gaps.
    pub use_gh_cli: bool,
    /// Per-page size on `/users/{user}/repos` (max 100).
    pub per_page: u32,
    /// `owner`, `all`, or `member`.
    pub repo_type: String,
    /// GitHub API base; override for GHES.
    pub api_base: String,
}

impl Default for GithubConfig {
    fn default() -> Self {
        Self {
            username: None,
            use_gh_cli: true,
            per_page: 100,
            repo_type: "owner".to_string(),
            api_base: "https://api.github.com".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Override for `~/.cache/nave`. `None` = use XDG default.
    pub root: Option<PathBuf>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self { root: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryConfig {
    /// Glob-like paths we care about, relative to repo root.
    /// We'll later support nested patterns; for now a literal-path match is enough.
    pub tracked_paths: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            tracked_paths: vec!["pyproject.toml".to_string()],
        }
    }
}

/// Load `NaveConfig` in the standard precedence stack.
///
/// `cli_overrides` should be a `Serialize` type holding any CLI-provided values.
/// Pass `()` if there are none.
pub fn load<T: Serialize>(cli_overrides: T) -> anyhow::Result<NaveConfig> {
    let mut figment = Figment::from(Serialized::defaults(NaveConfig::default()));

    let user_path = user_config_path()?;
    if user_path.exists() {
        figment = figment.merge(Toml::file(&user_path));
    }

    figment = figment
        .merge(Env::prefixed("NAVE_").split("__"))
        .merge(Serialized::defaults(cli_overrides));

    Ok(figment.extract()?)
}
