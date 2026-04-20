use anyhow::Result;
use clap::Args;

use nave_build::{BuildReport, GroupReport, HoleReport, SourceHint, run_build};
use nave_config::{NaveConfig, cache_root, load_default};

#[derive(Args, Debug)]
pub(crate) struct BuildArgs {
    /// Emit as JSON instead of text.
    #[arg(long)]
    pub json: bool,
    /// Restrict to groups whose pattern contains this substring.
    #[arg(long)]
    pub filter: Option<String>,
}

#[allow(clippy::unused_async)]
pub(crate) async fn run(args: BuildArgs) -> Result<()> {
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

    let mut report = run_build(&root, &cfg)?;

    if let Some(f) = &args.filter {
        report.groups.retain(|g| g.pattern.contains(f));
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_text(&report);
    }
    Ok(())
}

fn print_text(report: &BuildReport) {
    for group in &report.groups {
        print_group(group);
        println!();
    }
    for (pattern, count, reason) in &report.skipped {
        println!("skipped: {pattern}  ({count} files) — {reason}");
    }
}

fn print_group(g: &GroupReport) {
    println!("━━ {} ━━", g.pattern);
    println!("  instances: {}", g.instance_count);
    println!();
    println!("  template:");
    for line in g.template_text.lines() {
        println!("    {line}");
    }
    println!();
    if g.holes.is_empty() {
        println!("  (no holes — fleet is uniform at this path)");
        return;
    }
    println!("  holes:");
    for h in &g.holes {
        print_hole(h);
    }
}

fn print_hole(h: &HoleReport) {
    let presence = if h.present_in == h.total {
        format!("{}/{}", h.present_in, h.total)
    } else {
        format!("{}/{} optional", h.present_in, h.total)
    };
    let kind = format!("{:?}", h.kind).to_lowercase();
    let source = match &h.source_hint {
        SourceHint::Free => String::new(),
        SourceHint::DerivedFromRepoName => "  [derived: repo name]".to_string(),
        SourceHint::ConstantWhenPresent => "  [constant when present]".to_string(),
    };
    println!("    {}  [{}]  {}{}", h.address, kind, presence, source);
    for (val, count) in h.distinct_values.iter().take(8) {
        let short = short_value(val);
        println!("        {count}× {short}");
    }
    if h.distinct_values.len() > 8 {
        println!("        … {} more", h.distinct_values.len() - 8);
    }
}

fn short_value(v: &serde_json::Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    if s.len() > 80 {
        format!("{}…", &s[..77])
    } else {
        s
    }
}
