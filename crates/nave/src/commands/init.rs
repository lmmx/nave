use anyhow::{Context, Result};
use clap::Args;
use dialoguer::{Confirm, Input};
use tracing::{info, warn};

use nave_config::{NaveConfig, user_config_path};
use nave_github::auth::gh_username;

#[derive(Args, Debug)]
pub(crate) struct InitArgs {
    /// Accept all suggested defaults without prompting.
    #[arg(long)]
    pub no_interaction: bool,
    /// Overwrite an existing config without prompting.
    #[arg(long)]
    pub force: bool,
}

pub(crate) async fn run(args: InitArgs) -> Result<()> {
    let path = user_config_path()?;
    if path.exists() && !args.force {
        if args.no_interaction {
            warn!(path = %path.display(), "config already exists; pass --force to overwrite");
            return Ok(());
        }
        let overwrite = Confirm::new()
            .with_prompt(format!("{} already exists. Overwrite?", path.display()))
            .default(false)
            .interact()?;
        if !overwrite {
            info!("init cancelled");
            return Ok(());
        }
    }

    let mut cfg = NaveConfig::default();

    // Username: try gh first, then either ask or accept blank.
    let probed = gh_username().await;
    if args.no_interaction {
        cfg.github.username = probed;
    } else {
        let suggestion = probed.clone().unwrap_or_default();
        let prompt = if probed.is_some() {
            "GitHub username (detected via gh)"
        } else {
            "GitHub username (leave blank to set later)"
        };
        let name: String = Input::new()
            .with_prompt(prompt)
            .default(suggestion)
            .allow_empty(true)
            .interact_text()?;
        cfg.github.username = if name.trim().is_empty() {
            None
        } else {
            Some(name.trim().to_string())
        };
    }

    // per_page, use_gh_cli, tracked_paths: stick to defaults unless asked.
    if !args.no_interaction {
        cfg.github.use_gh_cli = Confirm::new()
            .with_prompt("Use `gh` CLI for auth and username probing?")
            .default(cfg.github.use_gh_cli)
            .interact()?;

        let per_page: String = Input::new()
            .with_prompt("Repos per page (max 100)")
            .default(cfg.github.per_page.to_string())
            .interact_text()?;
        if let Ok(n) = per_page.parse::<u32>() {
            cfg.github.per_page = n.clamp(1, 100);
        }
    }

    // Serialize and write.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(&cfg)?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    info!(path = %path.display(), "wrote config");
    Ok(())
}
