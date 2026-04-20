use anyhow::{Context, Result};
use clap::Args;
use tracing::info;

use nave_config::{NaveConfig, cache_root, load_default};
use nave_pull::run_pull;

#[derive(Args, Debug)]
pub(crate) struct PullArgs {}

pub(crate) async fn run(_args: PullArgs) -> Result<()> {
    let cfg: NaveConfig = load_default()?;
    let root = match cfg.cache.root.clone() {
        Some(r) => r,
        None => cache_root()?,
    };
    if !root.exists() {
        anyhow::bail!(
            "cache root {} does not exist; run `nave scan` first",
            root.display()
        );
    }

    let report = run_pull(&root)
        .await
        .with_context(|| format!("pulling into {}", root.display()))?;

    info!(
        cloned = report.cloned,
        updated = report.updated,
        recloned = report.recloned,
        skipped = report.skipped,
        failed = report.failed,
        sha_mismatches = report.sha_mismatches,
        "pull complete"
    );
    Ok(())
}
