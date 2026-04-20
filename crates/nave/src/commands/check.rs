use anyhow::Result;
use clap::Args;

use nave_check::{Totals, CheckReport, run_check};
use nave_config::{NaveConfig, cache_root, load_default};

#[derive(Args, Debug)]
pub(crate) struct CheckArgs {
    /// Emit results as JSON instead of text.
    #[arg(long)]
    pub json: bool,

    /// Only print failures (skip rows marked `ok`). Text mode only.
    #[arg(long)]
    pub failures_only: bool,
}

#[allow(clippy::unused_async)] // "Keep things consistent"
pub(crate) async fn run(args: CheckArgs) -> Result<()> {
    let cfg: NaveConfig = load_default()?;
    let root = match cfg.cache.root.clone() {
        Some(r) => r,
        None => cache_root()?,
    };
    if !root.exists() {
        anyhow::bail!(
            "cache root {} does not exist; run `nave scan` + `nave pull` first",
            root.display()
        );
    }

    let report = run_check(&root)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_text(&report, args.failures_only);
    }

    // Exit non-zero if there are any failures, to make CI usage natural.
    let failed = report.totals.drift
        + report.totals.parse_failed
        + report.totals.render_failed
        + report.totals.reparse_failed;
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn print_text(report: &CheckReport, failures_only: bool) {
    for r in &report.results {
        if failures_only && r.outcome == "ok" {
            continue;
        }
        let fmt = r.format.unwrap_or("?");
        match &r.detail {
            Some(d) => {
                println!(
                    "{:>14}  {}/{}  {}  [{}]  — {}",
                    r.outcome, r.owner, r.repo, r.path, fmt, d
                );
            }
            None => {
                println!(
                    "{:>14}  {}/{}  {}  [{}]",
                    r.outcome, r.owner, r.repo, r.path, fmt
                );
            }
        }
    }
    print_totals(&report.totals);
}

fn print_totals(t: &Totals) {
    println!();
    println!("── summary ──");
    println!("          ok  {}", t.ok);
    if t.drift > 0 {
        println!("       drift  {}", t.drift);
    }
    if t.parse_failed > 0 {
        println!("parse_failed  {}", t.parse_failed);
    }
    if t.render_failed > 0 {
        println!("render_fail   {}", t.render_failed);
    }
    if t.reparse_failed > 0 {
        println!("reparse_fail  {}", t.reparse_failed);
    }
    if t.unknown_format > 0 {
        println!("    unknown  {}", t.unknown_format);
    }
    if t.missing > 0 {
        println!("     missing  {}", t.missing);
    }
}
