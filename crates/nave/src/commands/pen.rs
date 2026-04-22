use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use tracing::info;

use nave_config::load_default;
use nave_pen::{CreateOptions, create_pen, list_pens, load_pen, remove_pen, resolve_pen_root};

#[derive(Debug, Args)]
pub(crate) struct PenArgs {
    #[command(subcommand)]
    pub action: PenAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PenAction {
    /// Create a new pen by filtering the fleet and cloning matching repos.
    Create(PenCreateArgs),
    /// List existing pens.
    List(PenListArgs),
    /// Remove a pen's local workspace and definition.
    Rm(PenRmArgs),
    /// Show a pen's full details.
    Show(PenShowArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PenCreateArgs {
    /// Optional pen name; auto-derived from the first term if omitted.
    #[arg(short, long)]
    pub name: Option<String>,
    /// Case-insensitive substring match (same as `nave search -i`).
    #[arg(short, long)]
    pub ignore_case: bool,
    /// One or more filter terms, same syntax as `nave search`.
    /// E.g. `workflow:maturin` or `pyproject:ruff`.
    #[arg(required = true, num_args = 1..)]
    pub terms: Vec<String>,
}

#[derive(Debug, Args, Default)]
pub(crate) struct PenListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PenRmArgs {
    pub name: String,
}

#[derive(Debug, Args)]
pub(crate) struct PenShowArgs {
    pub name: String,
    #[arg(long)]
    pub json: bool,
}

pub(crate) async fn run(args: PenArgs) -> Result<()> {
    match args.action {
        PenAction::Create(a) => run_create(a).await,
        PenAction::List(a) => run_list(a),
        PenAction::Rm(a) => run_rm(a),
        PenAction::Show(a) => run_show(a),
    }
}

async fn run_create(args: PenCreateArgs) -> Result<()> {
    let cfg = load_default()?;
    let opts = CreateOptions {
        name: args.name,
        terms: args.terms,
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

fn run_list(args: PenListArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let pens = list_pens(&root)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&pens)?);
    } else if pens.is_empty() {
        println!("(no pens)");
    } else {
        for p in &pens {
            println!(
                "{:<30} {:>3} repos  branch={}",
                p.name,
                p.repos.len(),
                p.branch
            );
        }
    }
    Ok(())
}

fn run_rm(args: PenRmArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    remove_pen(&root, &args.name).with_context(|| format!("removing pen {}", args.name))?;
    info!(name = %args.name, "pen removed");
    Ok(())
}

fn run_show(args: PenShowArgs) -> Result<()> {
    let cfg = load_default()?;
    let root = resolve_pen_root(&cfg.pen)?;
    let pen = load_pen(&root, &args.name)?;
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
            println!("  {}/{}  (branch={})", r.owner, r.name, r.default_branch);
        }
    }
    Ok(())
}
