use std::cmp::Reverse;

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};

use nave_config::{NaveConfig, Term, cache_root, load_default};
use nave_search::{SearchOptions, SearchReport, run_search};

#[derive(Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct SearchArgs {
    /// One or more search terms. Each is `[scope:]value[|value...]`.
    #[arg(required = true, num_args = 1..)]
    pub terms: Vec<String>,

    /// Structural predicate of the form `[scope:]path op literal`, where
    /// `op` is `=` (exact) or `~` (substring). Same syntax as
    /// `nave build --match`.
    #[arg(long = "match", value_name = "PREDICATE")]
    pub match_preds: Vec<String>,

    /// Output projection.
    #[arg(long, value_enum, default_value_t = Projection::Repos)]
    pub output: Projection,

    /// Emit JSON instead of the projected text form.
    #[arg(long)]
    pub json: bool,

    /// Print only the count of matches.
    #[arg(long)]
    pub count: bool,

    /// Show which files satisfied each term per repo.
    #[arg(long)]
    pub explain: bool,

    /// Case-insensitive substring match (ASCII).
    #[arg(short, long)]
    pub ignore_case: bool,

    /// Sort results by a key.
    #[arg(long, value_enum)]
    pub sort: Option<SortKey>,

    /// Limit to the first N results (applied after sorting).
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Projection {
    Repos,
    Files,
    Holes,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum SortKey {
    PushedAt,
    Name,
}

#[allow(clippy::unused_async)]
pub(crate) async fn run(args: SearchArgs) -> Result<()> {
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

    let terms: Vec<Term> = args
        .terms
        .iter()
        .map(|s| Term::parse(s).with_context(|| format!("parsing term {s:?}")))
        .collect::<Result<_>>()?;

    let match_preds: Vec<nave_config::MatchPredicate> = args
        .match_preds
        .iter()
        .map(|s| {
            nave_config::MatchPredicate::parse(s)
                .with_context(|| format!("parsing --match predicate {s:?}"))
        })
        .collect::<Result<_>>()?;

    let options = SearchOptions {
        terms,
        match_preds,
        ignore_case: args.ignore_case,
        enrich_holes: matches!(args.output, Projection::Holes),
    };

    let mut report = run_search(&root, &cfg, &options)?;

    apply_sort_and_limit(&mut report, args.sort, args.limit);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    if args.count {
        match args.output {
            Projection::Repos => println!("{}", report.repos.len()),
            Projection::Files => {
                let n: usize = report
                    .repos
                    .iter()
                    .flat_map(|r| &r.hits)
                    .flat_map(|h| &h.files)
                    .count();
                println!("{n}");
            }
            Projection::Holes => println!("{}", report.holes.len()),
        }
        return Ok(());
    }

    match args.output {
        Projection::Repos => print_repos(&report, args.explain),
        Projection::Files => print_files(&report, args.explain),
        Projection::Holes => print_holes(&report, args.explain),
    }

    Ok(())
}

fn apply_sort_and_limit(report: &mut SearchReport, sort: Option<SortKey>, limit: Option<usize>) {
    match sort {
        Some(SortKey::PushedAt) => {
            report.repos.sort_by_key(|r| Reverse(r.pushed_at));
        }
        Some(SortKey::Name) => {
            report.repos.sort_by(|a, b| {
                (a.owner.as_str(), a.repo.as_str()).cmp(&(b.owner.as_str(), b.repo.as_str()))
            });
        }
        None => {}
    }
    if let Some(n) = limit {
        report.repos.truncate(n);
    }
}

fn print_repos(report: &SearchReport, explain: bool) {
    for r in &report.repos {
        println!("{}/{}", r.owner, r.repo);
        if explain {
            for hit in &r.hits {
                for fm in &hit.files {
                    println!(
                        "    {}  →  {} (matched {:?})",
                        hit.term, fm.path, fm.matched_needle
                    );
                }
            }
        }
    }
}

fn print_files(report: &SearchReport, explain: bool) {
    for r in &report.repos {
        // De-duplicate across terms: a file that satisfies multiple
        // terms should only print once in the list, but --explain can
        // still show all the terms that matched it.
        use std::collections::BTreeMap;
        let mut by_path: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
        for hit in &r.hits {
            for fm in &hit.files {
                by_path
                    .entry(fm.path.as_str())
                    .or_default()
                    .push((hit.term.as_str(), fm.matched_needle.as_str()));
            }
        }
        for (path, terms) in by_path {
            println!("{}/{}:{}", r.owner, r.repo, path);
            if explain {
                for (term, needle) in terms {
                    println!("    {term} (matched {needle:?})");
                }
            }
        }
    }
}

fn print_holes(report: &SearchReport, explain: bool) {
    use std::collections::BTreeMap;

    // Group holes by (pattern, address) so the output shows the
    // structural positions as the primary unit, with repos as evidence.
    let mut by_addr: BTreeMap<(&str, &str), Vec<&nave_search::HoleHit>> = BTreeMap::new();
    for h in &report.holes {
        by_addr
            .entry((h.pattern.as_str(), h.address.as_str()))
            .or_default()
            .push(h);
    }

    for ((pattern, address), hits) in by_addr {
        println!("{pattern}  {address}  ({} hits)", hits.len());
        if explain {
            for h in hits {
                println!(
                    "    {}/{} :: {}  (needle: {:?})",
                    h.owner, h.repo, h.file_path, h.needle,
                );
                println!("        {}", h.snippet);
            }
        }
    }
}
