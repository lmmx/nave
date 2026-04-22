use anyhow::Result;
use clap::{Args, Subcommand};
use tracing::{info, warn};

use nave_config::{NaveConfig, cache_root, load_default};
use nave_schemas::{SchemaId, SchemaRegistry, schemas_for_tracked};

#[derive(Debug, Args)]
pub(crate) struct SchemasArgs {
    #[command(subcommand)]
    pub action: SchemasAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SchemasAction {
    /// Populate the schema cache based on tracked paths.
    Pull(PullArgs),
    /// List schemas and their cache status.
    List(ListArgs),
}

#[derive(Debug, Args, Default)]
pub(crate) struct PullArgs {
    /// Re-fetch all schemas even if cached.
    #[arg(long)]
    pub refresh: bool,
}

#[derive(Debug, Args, Default)]
pub(crate) struct ListArgs {
    #[arg(long)]
    pub json: bool,
}

pub(crate) async fn run(args: SchemasArgs) -> Result<()> {
    match args.action {
        SchemasAction::Pull(a) => run_pull(a).await,
        SchemasAction::List(a) => run_list(&a),
    }
}

pub(crate) async fn run_pull(args: PullArgs) -> Result<()> {
    let cfg = load_default()?;
    run_pull_with_config(&cfg, args.refresh).await
}

/// Entry point reusable by `nave init`. Never panics on network failure;
/// logs a warning and returns Ok so init can complete.
pub(crate) async fn run_pull_with_config(cfg: &NaveConfig, refresh: bool) -> Result<()> {
    let root = cache_root()?;
    let needed = schemas_for_tracked(&cfg.scan.tracked_paths);
    if needed.is_empty() {
        info!("no tracked paths require schemas; nothing to pull");
        return Ok(());
    }
    let ids: Vec<SchemaId> = needed.into_iter().collect();
    let reg = SchemaRegistry::new(root, cfg.schemas.clone())?;
    let result = if refresh {
        reg.refresh_all().await
    } else {
        reg.ensure_cached(&ids).await
    };
    if let Err(e) = result {
        warn!(error = %e, "schema pull failed");
        return Err(e);
    }
    info!(count = ids.len(), "schemas ready");
    Ok(())
}

fn run_list(args: &ListArgs) -> Result<()> {
    #[derive(serde::Serialize)]
    struct Row {
        id: &'static str,
        cached: bool,
        path: String,
        size_bytes: Option<u64>,
        source_url: Option<String>,
    }

    let cfg = load_default()?;
    let root = cache_root()?;
    let reg = SchemaRegistry::new(&root, cfg.schemas.clone())?;

    let rows: Vec<Row> = SchemaId::all()
        .iter()
        .map(|id| {
            let path = reg.schema_path(*id);
            let (cached, size) = match std::fs::metadata(&path) {
                Ok(m) => (true, Some(m.len())),
                Err(_) => (false, None),
            };
            Row {
                id: id.as_str(),
                cached,
                path: path.display().to_string(),
                size_bytes: size,
                source_url: cfg.schemas.sources.get(id.as_str()).cloned(),
            }
        })
        .collect();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        for r in &rows {
            let mark = if r.cached { "✓" } else { "·" };
            let size = r
                .size_bytes
                .map(|b| format!(" ({b} B)"))
                .unwrap_or_default();
            println!("{mark} {:<20} {}{}", r.id, r.path, size);
        }
    }
    Ok(())
}
