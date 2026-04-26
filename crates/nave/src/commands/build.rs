use anyhow::{Context, Result};
use clap::Args;

use nave_build::{BuildOptions, BuildReport, GroupReport, HoleReport, SourceHint, run_build};
use nave_config::{MatchPredicate, NaveConfig, Term, cache_root, load_default};

#[derive(Args, Debug)]
pub(crate) struct BuildArgs {
    /// Emit as JSON instead of text.
    #[arg(long)]
    pub json: bool,
    /// Restrict output to groups whose pattern contains this substring.
    #[arg(long)]
    pub filter: Option<String>,
    /// Search terms. Each is `[scope:]value[|value...]`.
    /// A scope restricts the term to files whose tracked-path pattern
    /// contains `scope` as a substring (e.g. `pyproject:`, `workflow:`,
    /// `dependabot:`), same as `nave search`.
    #[arg(long = "where", value_name = "TERM")]
    pub where_terms: Vec<String>,
    /// Structural predicate of the form `[scope:] [!] path [op literal]`,
    /// where `op` is one of `=`, `!=`, `^=`, `$=`, `*=`. A bare path
    /// tests presence; `!path` tests absence. Matches tree nodes whose
    /// relative `path` resolves to a scalar satisfying the comparison.
    /// Composes with `--where` and `--co-occur`.
    #[arg(long = "match", value_name = "PREDICATE")]
    pub match_preds: Vec<String>,
    /// Anti-unify the subtrees where `--where` terms co-occur rather
    /// than whole files. A co-occurrence site is the deepest non-root
    /// object ancestor shared by an anchor-term match and at least one
    /// match from each other term. Requires ≥ 2 `--where` terms.
    #[arg(long)]
    pub co_occur: bool,
    /// Only show profiles whose bindings overlap with holes that
    /// the --where/--match predicates would identify via co-occurrence.
    /// Requires at least one --where or --match term.
    #[arg(long)]
    pub relevant_profiles: bool,
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

    let where_terms: Vec<Term> = args
        .where_terms
        .iter()
        .map(|s| Term::parse(s).with_context(|| format!("parsing --where term {s:?}")))
        .collect::<Result<_>>()?;

    let match_preds: Vec<MatchPredicate> = args
        .match_preds
        .iter()
        .map(|s| {
            MatchPredicate::parse(s).with_context(|| format!("parsing --match predicate {s:?}"))
        })
        .collect::<Result<_>>()?;

    let report = run_build(
        &root,
        &cfg,
        &BuildOptions {
            where_terms,
            match_preds,
            co_occur: args.co_occur,
            filter: args.filter.clone(),
            relevant_profiles: args.relevant_profiles,
        },
    )?;

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
    // --- new: profiles section ---
    if !g.fca.profiles.is_empty() {
        let display_profiles = match &g.profile_match_preds {
            Some(preds) => nave_build::filter_profiles_by_predicates(&g.fca.profiles, preds),
            None => g.fca.profiles.clone(),
        };
        if !display_profiles.is_empty() {
            println!();
            println!(
                "  profiles: ({} concepts, {} non-trivial)",
                g.fca.total_concepts,
                display_profiles.len()
            );
            for (i, profile) in display_profiles.iter().enumerate() {
                let repo_names: Vec<&str> = profile
                    .instances
                    .iter()
                    .filter_map(|&idx| g.instances.get(idx).map(|r| r.repo.as_str()))
                    .collect();
                let repos_display = if repo_names.len() <= 4 {
                    repo_names.join(", ")
                } else {
                    format!(
                        "{}, … +{}",
                        repo_names[..3].join(", "),
                        repo_names.len() - 3
                    )
                };
                println!(
                    "    Profile {}  ({} repos: {})",
                    i + 1,
                    profile.support,
                    repos_display
                );
                for binding in &profile.bindings {
                    if binding.value.is_none() {
                        continue;
                    }
                    let val_str = serde_json::to_string(binding.value.as_ref().unwrap()).unwrap_or_default();
                    println!("      {} = {}", binding.address, val_str);
                }
            }
        }
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
