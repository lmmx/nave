//! Action manifest fetching and `with:` block validation.
//!
//! Tag/branch refs are resolved to a commit SHA via `git ls-remote` (one
//! round-trip, no clone). Content is cached by SHA, so tag moves naturally
//! produce a cache miss on the new SHA and a hit on the old one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use tokio::process::Command;
use tracing::debug;

use nave_config::cache::action_yml_path;

#[derive(Debug, Deserialize)]
pub struct ActionManifest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub inputs: BTreeMap<String, ActionInput>,
}

#[derive(Debug, Deserialize)]
pub struct ActionInput {
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    pub default: Option<String>,
    /// Present on actions that have deprecated this input.
    #[serde(rename = "deprecationMessage")]
    pub deprecation_message: Option<String>,
}

pub struct ActionRef<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    /// Whatever the workflow step wrote: `v1`, `v1.3.0`, a full SHA, `main`, etc.
    pub user_ref: &'a str,
}

pub struct FetchedAction {
    pub sha: String,
    pub path: PathBuf,
    pub manifest: ActionManifest,
}

/// Resolve ref → SHA, fetch action.yml if not cached, return parsed manifest.
pub async fn fetch_action(
    http: &reqwest::Client,
    cache_root: &Path,
    action: ActionRef<'_>,
) -> Result<FetchedAction> {
    let sha = if looks_like_sha(action.user_ref) {
        action.user_ref.to_string()
    } else {
        resolve_ref(action.owner, action.repo, action.user_ref).await?
    };

    let target = action_yml_path(cache_root, action.owner, action.repo, &sha);
    if !target.exists() {
        download_action_yml(http, action.owner, action.repo, &sha, &target).await?;
    }

    let bytes = std::fs::read(&target)?;
    let manifest: ActionManifest = serde_norway::from_slice(&bytes)?;
    Ok(FetchedAction {
        sha,
        path: target,
        manifest,
    })
}

fn looks_like_sha(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

async fn resolve_ref(owner: &str, repo: &str, r: &str) -> Result<String> {
    let url = format!("https://github.com/{owner}/{repo}.git");
    let out = Command::new("git")
        .args([
            "ls-remote",
            &url,
            r,
            &format!("refs/tags/{r}"),
            &format!("refs/heads/{r}"),
        ])
        .output()
        .await?;
    if !out.status.success() {
        bail!(
            "git ls-remote failed: {}",
            String::from_utf8_lossy(&out.stderr).trim(),
        );
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // Prefer annotated tag peel (`^{}`) if present, else first line.
    let peeled = text.lines().find(|l| l.contains("^{}"));
    let line = peeled
        .or_else(|| text.lines().next())
        .ok_or_else(|| anyhow!("no ref {r} on {owner}/{repo}"))?;
    let sha = line
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("malformed ls-remote output"))?;
    debug!(owner, repo, r, sha, "resolved ref");
    Ok(sha.to_string())
}

async fn download_action_yml(
    http: &reqwest::Client,
    owner: &str,
    repo: &str,
    sha: &str,
    dest: &Path,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    for name in ["action.yml", "action.yaml"] {
        let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/{sha}/{name}");
        let resp = http.get(&url).send().await?;
        if resp.status().is_success() {
            let bytes = resp.bytes().await?;
            std::fs::write(dest, &bytes)?;
            debug!(owner, repo, sha, name, "cached action manifest");
            return Ok(());
        }
        if resp.status() != reqwest::StatusCode::NOT_FOUND {
            bail!("fetching {url}: HTTP {}", resp.status());
        }
    }
    bail!("no action.yml/action.yaml at {owner}/{repo}@{sha}")
}

#[derive(Debug)]
pub struct InputCheck {
    pub missing_required: Vec<String>,
    pub unknown: Vec<String>,
    pub deprecated_used: Vec<(String, String)>,
}

impl InputCheck {
    pub fn is_ok(&self) -> bool {
        self.missing_required.is_empty() && self.unknown.is_empty()
    }
}

/// Check a `with:` block against an action's declared `inputs:`.
///
/// `provided` is expected to be the parsed `with:` mapping (a JSON object).
pub fn check_with_block(
    manifest: &ActionManifest,
    provided: &serde_json::Value,
) -> Result<InputCheck> {
    let map = provided
        .as_object()
        .ok_or_else(|| anyhow!("`with` block must be a mapping"))?;

    let mut missing = Vec::new();
    let mut deprecated = Vec::new();
    for (k, spec) in &manifest.inputs {
        let present = map.contains_key(k);
        if spec.required && spec.default.is_none() && !present {
            missing.push(k.clone());
        }
        if let (Some(msg), true) = (spec.deprecation_message.as_ref(), present) {
            deprecated.push((k.clone(), msg.clone()));
        }
    }
    let unknown: Vec<String> = map
        .keys()
        .filter(|k| !manifest.inputs.contains_key(k.as_str()))
        .cloned()
        .collect();
    Ok(InputCheck {
        missing_required: missing,
        unknown,
        deprecated_used: deprecated,
    })
}
