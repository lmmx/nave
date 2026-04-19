mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser, Debug)]
#[command(name = "nave", version, about = "Fleet ops for OSS package repos")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_env("NAVE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(false).init();

    let cli = Cli::parse();
    match cli.command {
        None => {
            nave_core::hello();
            Ok(())
        }
        Some(Command::Init(args)) => commands::init::run(args).await,
        Some(Command::Discover(args)) => commands::discover::run(args).await,
        Some(Command::Fetch(args)) => commands::fetch::run(args).await,
        Some(Command::Validate(args)) => commands::validate::run(args).await,
    }
}
