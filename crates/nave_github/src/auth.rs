//! Figure out how we can talk to the GitHub API.
//!
//! Priority:
//!   1. `NAVE_GITHUB_TOKEN` env var
//!   2. `gh auth token` (if `use_gh_cli` and gh is present and authed)
//!   3. anonymous (warn loudly; 60 req/hr)

use anyhow::Result;
use tokio::process::Command;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub enum AuthMode {
    Token { token: String, source: &'static str },
    Anonymous,
}

impl AuthMode {
    pub fn label(&self) -> &'static str {
        match self {
            AuthMode::Token { source, .. } => source,
            AuthMode::Anonymous => "anonymous",
        }
    }

    pub fn token(&self) -> Option<&str> {
        match self {
            AuthMode::Token { token, .. } => Some(token.as_str()),
            AuthMode::Anonymous => None,
        }
    }
}

pub async fn detect_auth(use_gh_cli: bool) -> AuthMode {
    if let Ok(tok) = std::env::var("NAVE_GITHUB_TOKEN") {
        if !tok.is_empty() {
            return AuthMode::Token {
                token: tok,
                source: "token_env",
            };
        }
    }

    if use_gh_cli {
        match gh_token().await {
            Ok(Some(tok)) => {
                return AuthMode::Token {
                    token: tok,
                    source: "gh",
                };
            }
            Ok(None) => debug!("gh CLI present but no token available"),
            Err(e) => debug!("gh CLI probe failed: {e}"),
        }
    }

    warn!("no GitHub auth available; proceeding anonymously (60 req/hr limit)");
    AuthMode::Anonymous
}

/// Returns `Ok(None)` if gh is not installed or not authed.
async fn gh_token() -> Result<Option<String>> {
    let out = match Command::new("gh").arg("auth").arg("token").output().await {
        Ok(o) => o,
        Err(_) => return Ok(None), // gh not on PATH
    };
    if !out.status.success() {
        return Ok(None);
    }
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if token.is_empty() {
        Ok(None)
    } else {
        Ok(Some(token))
    }
}

/// Probe the username via `gh auth status --json hosts --jq '.hosts .[][0] .login'`.
/// Returns `None` if gh is absent or not authed.
pub async fn gh_username() -> Option<String> {
    let out = Command::new("gh")
        .args([
            "auth",
            "status",
            "--json",
            "hosts",
            "--jq",
            ".hosts .[][0] .login",
        ])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}
