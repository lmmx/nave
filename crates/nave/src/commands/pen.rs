use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use regex::Regex;
use tracing::info;

use nave_config::{cache_root, load_default};
use nave_pen::{
    CreateOptions, Divergence, Freshness, RepoState, RunState, WorkTree, clean_pen,
    compute_repo_state, create_pen, exec_pen, list_pens, load_pen, reinit_pen, remove_pen_safe,
    resolve_pen_root, revert_pen, sync_pen,
};

#[derive(Debug, Args)]
pub(crate) struct PenArgs {
    #[command(subcommand)]
    pub action: PenAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PenAction {
    Create(PenCreateArgs),
    List(PenListArgs),
    Show(PenShowArgs),
    Status(PenStatusArgs),
    Sync(PenSyncArgs),
    Clean(PenSimpleArgs),
    Revert(PenAllowDirtyArgs),
    Reinit(PenAllowDirtyArgs),
    Exec(PenExecArgs),
    Rm(PenRmArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PenCreateArgs {
    #[arg(short, long)]
    pub name: Option<String>,
    #[arg(short, long)]
    pub ignore_case: bool,
    /// Structural predicate (see `nave build --match`).
    #[arg(long = "match", value_name = "PREDICATE")]
    pub match_preds: Vec<String>,
    /// Filter terms (see `nave search`).
    #[arg(required = true, num_args = 1..)]
    pub terms: Vec<String>,
}

#[derive(Debug, Args, Default)]
pub(crate) struct PenListArgs {
    /// docker-style `key=value` filters over state. Keys: `working-tree`,
    /// `freshness`, `run-state`, `divergence`. Multiple allowed.
    #[arg(short = 'f', long = "filter", value_name = "KEY=VALUE")]
    pub filters: Vec<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenShowArgs {
    /// Pen name, or empty when `--filter` is used.
    #[arg(default_value = "")]
    pub name: String,
    /// Regex over pen name; must match exactly one pen.
    #[arg(long)]
    pub filter: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenStatusArgs {
    pub name: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenSyncArgs {
    pub name: String,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenSimpleArgs {
    pub name: String,
}

#[derive(Debug, Args)]
pub(crate) struct PenAllowDirtyArgs {
    pub name: String,
    #[arg(long)]
    pub allow_dirty: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenExecArgs {
    pub name: String,
    /// Only run in this repo (matches bare name or `owner/name`).
    #[arg(long)]
    pub only: Option<String>,
    /// Commit any changes after running.
    #[arg(long)]
    pub commit: bool,
    /// Commit and push.
    #[arg(long = "push-changes")]
    pub push_changes: bool,
    /// Commit message.
    #[arg(short = 'm', long)]
    pub message: Option<String>,
    /// The command to execute after `--`.
    #[arg(last = true, required = true)]
    pub cmd: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct PenRmArgs {
    pub name: String,
    #[arg(long)]
    pub allow_dirty: bool,
}

pub(crate) async fn run(args: PenArgs) -> Result<()> {
    match args.action {
        PenAction::Create(a) => run_create(a).await,
        PenAction::List(a) => run_list(a).await,
        PenAction::Show(a) => run_show(&a),
        PenAction::Status(a) => run_status(a).await,
        PenAction::Sync(a) => run_sync(a).await,
        PenAction::Clean(a) => run_clean(a).await,
        PenAction::Revert(a) => run_revert(a).await,
        PenAction::Reinit(a) => run_reinit(a).await,
        PenAction::Exec(a) => run_exec(a).await,
        PenAction::Rm(a) => run_rm(a).await,
    }
}

async fn run_create(args: PenCreateArgs) -> Result<()> {
    let cfg = load_default()?;
    let opts = CreateOptions {
        name: args.name,
        terms: args.terms,
        match_preds: args.match_preds,
        ignore_case: args.ignore_case,
    };
    let pen = create_pen(&cfg, opts).await?;
    info!(name = %pen.name, repos = pen.repos.len(), "pen created");
    println!("{}", pen.name);
    for r in &pen.repos {
        println!("  {}/{}", r.owner, r.name);
    }
    Ok(())
}

async fn run_list(args: PenListArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let cache = cache_root()?;
    let pens = list_pens(&root)?;
    let filters = parse_filters(&args.filters)?;

    // Compute per-pen summaries; each pen gets a vector of repo states.
    let mut rows: Vec<(String, usize, Vec<RepoState>)> = Vec::new();
    for p in &pens {
        let mut states = Vec::with_capacity(p.repos.len());
        for r in &p.repos {
            states.push(compute_repo_state(&root, &cache, p, r).await?);
        }
        if !filters.all_match(&states) {
            continue;
        }
        rows.push((p.name.clone(), p.repos.len(), states));
    }

    if args.json {
        #[derive(serde::Serialize)]
        struct Row<'a> {
            name: &'a str,
            repos: usize,
            states: &'a [RepoState],
        }
        let json_rows: Vec<Row> = rows
            .iter()
            .map(|(n, c, s)| Row {
                name: n,
                repos: *c,
                states: s,
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_rows)?);
    } else if rows.is_empty() {
        println!("(no pens)");
    } else {
        for (name, count, states) in &rows {
            let dirty = states
                .iter()
                .filter(|s| s.working_tree == WorkTree::Dirty)
                .count();
            let stale = states
                .iter()
                .filter(|s| s.freshness == Freshness::Stale)
                .count();
            let run = states
                .iter()
                .filter(|s| s.run_state != RunState::NotRun)
                .count();
            println!(
                "{name:<30} {count:>3} repos  dirty={dirty}/{count} stale={stale}/{count} run={run}/{count}",
            );
        }
    }
    Ok(())
}

fn run_show(args: &PenShowArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let pen = if let Some(rx) = &args.filter {
        let re = Regex::new(rx).with_context(|| format!("invalid regex {rx:?}"))?;
        let all = list_pens(&root)?;
        let matched: Vec<_> = all.into_iter().filter(|p| re.is_match(&p.name)).collect();
        match matched.len() {
            0 => bail!("no pen matched regex {rx:?}"),
            1 => matched.into_iter().next().unwrap(),
            n => bail!("{n} pens matched regex {rx:?}; narrow the pattern"),
        }
    } else {
        if args.name.is_empty() {
            bail!("pen name or --filter required");
        }
        load_pen(&root, &args.name)?
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&pen)?);
    } else {
        println!("name: {}", pen.name);
        println!("branch: {}", pen.branch);
        println!("created: {}", pen.created_at);
        if !pen.filter.terms.is_empty() {
            println!("filter: {:?}", pen.filter.terms);
        }
        println!("repos ({}):", pen.repos.len());
        for r in &pen.repos {
            println!(
                "  {}/{}  branch={}  synced={}",
                r.owner, r.name, r.default_branch, r.synced_at
            );
        }
    }
    Ok(())
}

async fn run_status(args: PenStatusArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let cache = cache_root()?;
    let pen = load_pen(&root, &args.name)?;

    let mut states = Vec::with_capacity(pen.repos.len());
    for r in &pen.repos {
        states.push(compute_repo_state(&root, &cache, &pen, r).await?);
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&states)?);
    } else {
        for s in &states {
            let div = match s.divergence {
                Divergence::UpToDate => "up-to-date".to_string(),
                Divergence::Ahead => format!("ahead {}", s.ahead),
                Divergence::Behind => format!("behind {}", s.behind),
                Divergence::Diverged => format!("diverged {}/{}", s.ahead, s.behind),
                Divergence::Unknown => "unknown".to_string(),
            };
            println!(
                "{}/{:<30}  tree={:<7}  fresh={:<7}  run={:<10}  {}",
                s.owner,
                s.repo,
                format!("{:?}", s.working_tree).to_lowercase(),
                format!("{:?}", s.freshness).to_lowercase(),
                format!("{:?}", s.run_state).to_lowercase(),
                div,
            );
        }
    }
    Ok(())
}

async fn run_sync(args: PenSyncArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let cache = cache_root()?;
    let mut pen = load_pen(&root, &args.name)?;
    let report = sync_pen(&root, &cache, &mut pen, args.dry_run).await?;
    if args.dry_run {
        if report.stale_repos.is_empty() {
            println!("all fresh");
        } else {
            println!("stale ({}):", report.stale_repos.len());
            for s in &report.stale_repos {
                println!("  {s}");
            }
        }
    } else {
        info!(freshened = report.freshened, "sync complete");
    }
    Ok(())
}

async fn run_clean(args: PenSimpleArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let pen = load_pen(&root, &args.name)?;
    clean_pen(&root, &pen).await
}

async fn run_revert(args: PenAllowDirtyArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let pen = load_pen(&root, &args.name)?;
    revert_pen(&root, &pen, args.allow_dirty).await
}

async fn run_reinit(args: PenAllowDirtyArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let pen = load_pen(&root, &args.name)?;
    reinit_pen(&root, &pen, args.allow_dirty).await
}

async fn run_exec(args: PenExecArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let pen = load_pen(&root, &args.name)?;
    let push = args.push_changes;
    let commit = args.commit || push;
    exec_pen(
        &root,
        &pen,
        &args.cmd,
        args.only.as_deref(),
        commit,
        push,
        args.message.as_deref(),
    )
    .await
}

async fn run_rm(args: PenRmArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let pen = load_pen(&root, &args.name)?;
    remove_pen_safe(&root, &pen, args.allow_dirty).await?;
    info!(name = %args.name, "pen removed");
    Ok(())
}

// --- filter parsing for `list --filter key=value` ---

struct ListFilters {
    working_tree: Option<WorkTree>,
    freshness: Option<Freshness>,
    run_state: Option<RunState>,
}

impl ListFilters {
    fn all_match(&self, states: &[RepoState]) -> bool {
        if states.is_empty() {
            // A filter on an empty pen only matches if no filter is set.
            return self.working_tree.is_none()
                && self.freshness.is_none()
                && self.run_state.is_none();
        }
        if let Some(wt) = self.working_tree
            && !states.iter().any(|s| s.working_tree == wt)
        {
            return false;
        }
        if let Some(f) = self.freshness
            && !states.iter().any(|s| s.freshness == f)
        {
            return false;
        }
        if let Some(r) = self.run_state
            && !states.iter().any(|s| s.run_state == r)
        {
            return false;
        }
        true
    }
}

fn parse_filters(raw: &[String]) -> Result<ListFilters> {
    let mut f = ListFilters {
        working_tree: None,
        freshness: None,
        run_state: None,
    };
    for s in raw {
        let (k, v) = s
            .split_once('=')
            .with_context(|| format!("filter must be key=value: {s:?}"))?;
        match k {
            "working-tree" => {
                f.working_tree = Some(match v {
                    "clean" => WorkTree::Clean,
                    "dirty" => WorkTree::Dirty,
                    "missing" => WorkTree::Missing,
                    _ => bail!("unknown working-tree value {v:?}"),
                });
            }
            "freshness" => {
                f.freshness = Some(match v {
                    "fresh" => Freshness::Fresh,
                    "stale" => Freshness::Stale,
                    _ => bail!("unknown freshness value {v:?}"),
                });
            }
            "run-state" => {
                f.run_state = Some(match v {
                    "not-run" => RunState::NotRun,
                    "run-local" => RunState::RunLocal,
                    "run-pushed" => RunState::RunPushed,
                    _ => bail!("unknown run-state value {v:?}"),
                });
            }
            _ => bail!("unknown filter key {k:?}"),
        }
    }
    Ok(f)
}
