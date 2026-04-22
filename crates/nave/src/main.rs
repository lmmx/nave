mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser, Debug)]
#[command(
    name = "nave",
    version,
    about = "Fleet ops for OSS package repos",
    subcommand_required = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Interactively create `~/.config/nave.toml`.
    Init(commands::init::InitArgs),
    /// List a user's repos and cache the set of tracked files.
    Scan(commands::scan::ScanArgs),
    /// Sparse-checkout scanned repos into the cache.
    Pull(commands::pull::PullArgs),
    /// Check tracked configs parse and round-trip cleanly.
    Check(commands::check::CheckArgs),
    /// Simplify configs across repos into shared templates.
    Build(commands::build::BuildArgs),
    /// Validate schemas for tracked files.
    Schemas(commands::schemas::SchemasArgs),
    /// Search cached repos for substring patterns across tracked files.
    Search(commands::search::SearchArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_env("NAVE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(false).init();

    let cli = Cli::parse();
    match cli.command {
        Command::Init(args) => commands::init::run(args).await,
        Command::Scan(args) => commands::scan::run(args).await,
        Command::Pull(args) => commands::pull::run(args).await,
        Command::Check(args) => commands::check::run(args).await,
        Command::Build(args) => commands::build::run(args).await,
        Command::Schemas(args) => commands::schemas::run(args).await,
        Command::Search(args) => commands::search::run(args).await,
    }
}
