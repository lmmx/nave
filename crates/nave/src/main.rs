use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "nave", version, about = "Fleet ops for OSS package repos")]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    nave_core::hello();
}
