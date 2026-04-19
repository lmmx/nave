use anyhow::{Context, Result};
use clap::Args;
use tracing::info;

use nave_config::{NaveConfig, cache_root, load};
use nave_fetch::run_fetch;

#[derive(Args, Debug)]
pub(crate) struct FetchArgs {}

pub(crate) async fn run(_args: FetchArgs) -> Result<()> {
    let cfg: NaveConfig = load(())?;
    let root = match cfg.cache.root.clone() {
        Some(r) => r,
        None => cache_root()?,
    };
    if !root.exists() {
        anyhow::bail!(
            "cache root {} does not exist; run `nave discover` first",
            root.display()
        );
    }

    let report = run_fetch(&root)
        .await
        .with_context(|| format!("fetching into {}", root.display()))?;

    info!(
        cloned = report.cloned,
        updated = report.updated,
        recloned = report.recloned,
        skipped = report.skipped,
        failed = report.failed,
        sha_mismatches = report.sha_mismatches,
        "fetch complete"
    );
    Ok(())
}
