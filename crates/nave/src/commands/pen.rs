use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use regex::Regex;
use tracing::info;

use nave_config::{cache_root, load_default};
use nave_pen::{
    CreateOptions, Divergence, Freshness, RepoState, RewriteOptions, RunState, WorkTree, clean_pen,
    compute_repo_state, create_pen, exec_pen, list_pens, load_pen, reinit_pen, remove_pen_safe,
    resolve_pen_root, revert_pen, rewrite_pen, sync_pen,
};

#[derive(Debug, Args)]
pub(crate) struct PenArgs {
    #[command(subcommand)]
    pub action: PenAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PenAction {
    /// Create a pen by filtering the fleet and cloning matching repos.
    Create(PenCreateArgs),
    /// List pens, optionally filtered by state.
    List(PenListArgs),
    /// Show a single pen's details.
    Show(PenShowArgs),
    /// Show per-repo state for a pen: working tree, freshness, run state, divergence.
    Status(PenStatusArgs),
    /// Refresh a pen's synced baseline against the fleet cache.
    Sync(PenSyncArgs),
    /// Discard uncommitted working-tree changes across a pen's repos.
    Clean(PenSimpleArgs),
    /// Drop local commits on the pen branch, returning to the synced baseline.
    Revert(PenAllowDirtyArgs),
    /// Rebuild the pen branch from origin's default branch.
    Reinit(PenAllowDirtyArgs),
    /// Run a command in each pen repo, optionally committing/pushing changes.
    Exec(PenExecArgs),
    /// Remove a pen's local workspace and definition.
    Rm(PenRmArgs),
    /// Apply declarative rewrites defined in the pen's `pen.toml`.
    Rewrite(PenRewriteArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PenCreateArgs {
    /// Explicit pen name. Defaults to `nave/<slug>` derived from the first term.
    #[arg(short, long)]
    pub name: Option<String>,
    /// Treat filter terms case-insensitively (same as `nave search -i`).
    #[arg(short, long)]
    pub ignore_case: bool,
    /// Structural predicate to narrow the repo set, of the form
    /// `[scope:]path op literal`, where `op` is one of `=`, `!=`, `^=`, `$=`, `*=`.
    /// Same syntax as `nave build --match`.
    #[arg(long = "match", value_name = "PREDICATE")]
    pub match_preds: Vec<String>,
    /// Filter terms. Each is `[scope:]value[|value...]`.
    /// A scope restricts the term to files whose tracked-path pattern
    /// contains `scope` as a substring (e.g. `pyproject:`, `workflow:`,
    /// `dependabot:`).
    #[arg(num_args = 0..)]
    pub terms: Vec<String>,
}

#[derive(Debug, Args, Default)]
pub(crate) struct PenListArgs {
    /// Filter by state. Keys: `working-tree`, `freshness`,
    /// `run-state`. Values are the state labels (e.g. `dirty`, `stale`, `run-local`).
    /// Multiple allowed.
    #[arg(short = 'f', long = "filter", value_name = "KEY=VALUE")]
    pub filters: Vec<String>,
    /// Emit JSON instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenShowArgs {
    /// Pen name, or empty when `--filter` is used.
    #[arg(default_value = "")]
    pub name: String,
    /// Regex over pen names. Must match exactly one pen.
    #[arg(long)]
    pub filter: Option<String>,
    /// Emit JSON instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenStatusArgs {
    /// Pen name.
    pub name: String,
    /// Emit JSON instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenSyncArgs {
    /// Pen name.
    pub name: String,
    /// Report what would change without touching anything.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenSimpleArgs {
    /// Pen name.
    pub name: String,
}

#[derive(Debug, Args)]
pub(crate) struct PenAllowDirtyArgs {
    /// Pen name.
    pub name: String,
    /// Discard uncommitted working-tree changes before proceeding.
    /// Without this, dirty repos cause the command to abort.
    #[arg(long)]
    pub allow_dirty: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenExecArgs {
    /// Pen name.
    pub name: String,
    /// Restrict execution to a single repo, matched by bare name or `owner/name`.
    #[arg(long)]
    pub only: Option<String>,
    /// Commit any changes after running the command.
    #[arg(long)]
    pub commit: bool,
    /// Commit and push to `origin/<pen-branch>`. Implies `--commit`.
    #[arg(long = "push-changes")]
    pub push_changes: bool,
    /// Commit message. Defaults to "nave pen exec".
    #[arg(short = 'm', long)]
    pub message: Option<String>,
    /// The command to execute. Everything after `--` is passed through.
    #[arg(last = true, required = true)]
    pub cmd: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct PenRmArgs {
    /// Pen name.
    pub name: String,
    /// Remove the pen even if any repo has uncommitted changes.
    #[arg(long)]
    pub allow_dirty: bool,
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PenRewriteArgs {
    /// Pen name.
    pub name: String,
    /// Restrict to a single repo (bare name or owner/name).
    #[arg(long)]
    pub only: Option<String>,
    /// Restrict to specific op ids. Repeatable; default = all not-yet-applied ops.
    #[arg(long = "op")]
    pub ops: Vec<String>,
    /// Plan and validate without writing.
    #[arg(long)]
    pub dry_run: bool,
    /// Compute and print unified diffs (implies --dry-run).
    #[arg(long)]
    pub diff: bool,
    /// Bypass the dirty-tree gate.
    #[arg(long)]
    pub allow_dirty: bool,
    /// Skip post-mutation schema validation.
    #[arg(long)]
    pub no_validate: bool,
    /// Re-run ops already marked applied for a repo.
    #[arg(long)]
    pub force: bool,
    /// Disable per-repo atomic rollback. Failed rewrites leave partial
    /// changes in the working tree.
    #[arg(long)]
    pub no_rollback: bool,
    /// Emit JSON report.
    #[arg(long)]
    pub json: bool,
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
        PenAction::Rewrite(a) => run_rewrite(a).await,
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
async fn run_rewrite(args: PenRewriteArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let mut pen = load_pen(&root, &args.name)?;
    let report = rewrite_pen(
        &root,
        &cfg,
        &mut pen,
        RewriteOptions {
            only: args.only,
            op_ids: args.ops,
            dry_run: args.dry_run || args.diff,
            diff: args.diff,
            allow_dirty: args.allow_dirty,
            no_validate: args.no_validate,
            force: args.force,
            no_rollback: args.no_rollback,
        },
    )
    .await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_rewrite_report(&report);
    }

    let any_failed = report.repos.iter().any(|r| r.rollback_trigger.is_some());
    if any_failed {
        std::process::exit(1);
    }
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

fn print_rewrite_report(report: &nave_pen::RewritePenReport) {
    println!("pen: {}  run: {}", report.pen, report.run_id);
    if report.dry_run {
        println!("(dry-run)");
    }
    for r in &report.repos {
        let status = if r.committed {
            "✓"
        } else if r.rollback_trigger.is_some() {
            "✗"
        } else {
            "·"
        };
        println!("{status} {}/{}", r.owner, r.repo);
        for o in &r.ops {
            let label = match &o.outcome {
                nave_rewrite::OpOutcome::Applied => "applied".to_string(),
                nave_rewrite::OpOutcome::NoTargets => "no-targets".to_string(),
                nave_rewrite::OpOutcome::Failed { reason } => format!("failed: {reason}"),
                nave_rewrite::OpOutcome::ValidationFailed { errors } => {
                    format!("validation failed ({} errors)", errors.len())
                }
            };
            println!("    {} — {label}", o.op_id);
            for f in &o.files {
                println!("        {f}");
            }
        }
        if let Some(trigger) = &r.rollback_trigger {
            match &r.logs_dir {
                Some(p) => println!(
                    "  ↪ rolled back due to op {trigger:?}; see logs at {}",
                    p.display()
                ),
                None => println!("  ↪ rolled back due to op {trigger:?}"),
            }
        }
        for d in &r.diffs {
            println!("--- diff: {} ---", d.path);
            print!("{}", d.diff);
        }
    }
    println!();
    println!("op statuses:");
    for (id, s) in &report.op_statuses {
        println!("  {id}: {s:?}");
    }
}
