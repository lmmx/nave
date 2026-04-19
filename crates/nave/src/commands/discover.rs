use anyhow::{Context, Result, bail};
use clap::Args;
use tracing::info;

use nave_config::{NaveConfig, cache_root, load, user_config_path};
use nave_discover::run_discovery;
use nave_github::auth::gh_username;

#[derive(Args, Debug)]
pub(crate) struct DiscoverArgs {
    /// Override the GitHub username for this run.
    #[arg(long)]
    pub user: Option<String>,
    /// Don't prompt; fail fast if anything is missing.
    #[arg(long)]
    pub no_interaction: bool,
}

pub(crate) async fn run(args: DiscoverArgs) -> Result<()> {
    let cfg: NaveConfig = load(())?;

    // Resolve username: CLI > config > gh > error.
    let username = resolve_username(&cfg, args.user.as_deref(), args.no_interaction).await?;

    // If CLI provided a username and the user config had none, persist it.
    if args.user.is_some()
        && cfg.github.username.is_none()
        && let Err(e) = persist_username(&username)
    {
        tracing::warn!("could not persist username to user config: {e}");
    }

    let root = match cfg.cache.root.clone() {
        Some(r) => r,
        None => cache_root()?,
    };
    std::fs::create_dir_all(&root)
        .with_context(|| format!("creating cache root {}", root.display()))?;

    let report = run_discovery(&cfg, &root, &username).await?;
    info!(
        repos = report.repos_seen,
        with_tracked = report.repos_with_tracked_files,
        tracked_files = report.tracked_file_count,
        auth = %report.auth_mode,
        incremental = report.incremental,
        "discovery complete"
    );
    Ok(())
}

async fn resolve_username(
    cfg: &NaveConfig,
    cli: Option<&str>,
    no_interaction: bool,
) -> Result<String> {
    if let Some(u) = cli {
        return Ok(u.to_string());
    }
    if let Some(u) = cfg.github.username.as_ref() {
        return Ok(u.clone());
    }
    if cfg.github.use_gh_cli
        && let Some(u) = gh_username().await
    {
        return Ok(u);
    }
    if no_interaction {
        bail!(
            "no GitHub username available (pass --user, set github.username in ~/.config/nave.toml, or authenticate gh)"
        );
    }
    // Last resort: prompt.
    let name: String = dialoguer::Input::new()
        .with_prompt("GitHub username")
        .allow_empty(false)
        .interact_text()?;
    Ok(name.trim().to_string())
}

/// Best-effort: write username into the user config file. Non-fatal on failure.
fn persist_username(username: &str) -> Result<()> {
    let path = user_config_path()?;
    let mut cfg: NaveConfig = if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).unwrap_or_default()
    } else {
        NaveConfig::default()
    };
    cfg.github.username = Some(username.to_string());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(&cfg)?;
    std::fs::write(&path, text)?;
    Ok(())
}
