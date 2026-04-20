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
    Discover(commands::discover::DiscoverArgs),
    /// Sparse-checkout discovered repos into the cache.
    Fetch(commands::fetch::FetchArgs),
    /// Validate tracked configs parse and round-trip cleanly.
    Validate(commands::validate::ValidateArgs),
    /// Anti-unify tracked configs across repos to discover shared templates.
    Distil(commands::distil::DistilArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_env("NAVE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(false).init();

    let cli = Cli::parse();
    match cli.command {
        Command::Init(args) => commands::init::run(args).await,
        Command::Discover(args) => commands::discover::run(args).await,
        Command::Fetch(args) => commands::fetch::run(args).await,
        Command::Validate(args) => commands::validate::run(args).await,
        Command::Distil(args) => commands::distil::run(args).await,
    }
}
