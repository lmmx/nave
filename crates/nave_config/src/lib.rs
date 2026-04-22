//! Configuration layering for nave.
//!
//! Precedence (lowest → highest):
//!   1. Baked-in defaults (`NaveConfig::default`)
//!   2. User config at `~/.config/nave.toml`
//!   3. Environment variables prefixed `NAVE_`
//!   4. CLI overrides supplied by the binary

pub mod address;
pub mod cache;
pub mod match_pred;
pub mod matcher;
pub mod paths;
pub mod term;

use std::path::PathBuf;

use figment2::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

pub use crate::address::{Match, find_addresses, walk_matches};
pub use crate::match_pred::{MatchPredicate, Op as MatchOp, find_match_addresses};
pub use crate::matcher::PathMatcher;
pub use crate::paths::{cache_root, user_config_path};
pub use crate::term::Term;

/// The fully-resolved nave configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NaveConfig {
    pub github: GithubConfig,
    pub cache: CacheConfig,
    pub scan: ScanConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GithubConfig {
    pub username: Option<String>,
    pub use_gh_cli: bool,
    pub per_page: u32,
    pub repo_type: String,
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
    pub root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    pub tracked_paths: Vec<String>,
    pub case_insensitive: bool,
    pub exclude_forks: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            tracked_paths: default_tracked_paths(),
            case_insensitive: true,
            exclude_forks: true,
        }
    }
}

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

pub fn load_default() -> anyhow::Result<NaveConfig> {
    let mut figment = Figment::from(Serialized::defaults(NaveConfig::default()));

    let user_path = user_config_path()?;
    if user_path.exists() {
        figment = figment.merge(Toml::file(&user_path));
    }

    figment = figment.merge(Env::prefixed("NAVE_").split("__"));

    Ok(figment.extract()?)
}

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
