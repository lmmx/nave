//! Configuration layering for nave.
//!
//! Precedence (lowest → highest):
//!   1. Baked-in defaults (`NaveConfig::default`)
//!   2. User config at `~/.config/nave.toml`
//!   3. Environment variables prefixed `NAVE_`
//!   4. CLI overrides supplied by the binary

pub mod cache;
pub mod matcher;
pub mod paths;

use std::path::PathBuf;

use figment2::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

pub use crate::matcher::PathMatcher;
pub use crate::paths::{cache_root, user_config_path};

/// The fully-resolved nave configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NaveConfig {
    pub github: GithubConfig,
    pub cache: CacheConfig,
    pub discovery: DiscoveryConfig,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Override for `~/.cache/nave`. `None` = use XDG default.
    pub root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryConfig {
    /// Glob patterns for files to track, relative to repo root.
    ///
    /// Globs follow gitignore-ish semantics:
    ///   - `*` matches within a path component (not crossing `/`)
    ///   - `**` matches zero or more components
    ///   - `?`, `[abc]`, `{a,b}` work as expected
    ///
    /// Defaults cover: `pyproject.toml`, `Cargo.toml`, pre-commit configs,
    /// `.github/workflows/*`, and the top-level dependabot config.
    pub tracked_paths: Vec<String>,

    /// Match paths case-insensitively. Defaults to true to catch typos like
    /// `Pyproject.toml`; most real configs are lowercase.
    pub case_insensitive: bool,

    /// Exclude forks from discovery. Defaults to true — forks typically inherit
    /// upstream's configs and we'd rather model the canonical source.
    pub exclude_forks: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            tracked_paths: default_tracked_paths(),
            case_insensitive: true,
            exclude_forks: true,
        }
    }
}

/// The canonical default list of tracked file patterns. Public so `init`
/// and other tools can show it to the user verbatim.
pub fn default_tracked_paths() -> Vec<String> {
    vec![
        "pyproject.toml".to_string(),
        "Cargo.toml".to_string(),
        ".pre-commit-config.yaml".to_string(),
        ".pre-commit-config.yml".to_string(),
        ".github/workflows/*.yml".to_string(),
        ".github/workflows/*.yaml".to_string(),
        ".github/dependabot.yml".to_string(),
        ".github/dependabot.yaml".to_string(),
    ]
}

/// Load with no CLI overrides.
pub fn load_default() -> anyhow::Result<NaveConfig> {
    let mut figment = Figment::from(Serialized::defaults(NaveConfig::default()));

    let user_path = user_config_path()?;
    if user_path.exists() {
        figment = figment.merge(Toml::file(&user_path));
    }

    figment = figment.merge(Env::prefixed("NAVE_").split("__"));

    Ok(figment.extract()?)
}

/// Load `NaveConfig` in the standard precedence stack, with CLI overrides
/// (must serialize to a map, e.g. a struct with `#[derive(Serialize)]`).
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
