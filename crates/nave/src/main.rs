use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "nave", version, about = "Fleet ops for OSS package repos")]
struct Cli {}

fn main() -> Result<()> {
    let _cli = Cli::parse();
    nave_core::hello();
    Ok(())
}