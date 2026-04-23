use anyhow::Result;
use clap::{Args, Subcommand};
use tracing::{info, warn};

use nave_config::{NaveConfig, cache_root, load_default};
use nave_schemas::{SchemaId, SchemaRegistry, schemas_for_tracked};

#[derive(Debug, Args)]
/// Manage the JSON Schema cache and validate tracked files.
pub(crate) struct SchemasArgs {
    #[command(subcommand)]
    pub action: SchemasAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SchemasAction {
    /// Populate the schema cache based on tracked paths.
    Pull(PullArgs),
    /// List schemas and their cache status.
    List(ListArgs),
    /// Validate tracked files in a pen against their schemas.
    Validate(ValidateArgs),
}

#[derive(Debug, Args, Default)]
pub(crate) struct PullArgs {
    /// Re-fetch all schemas even if cached.
    #[arg(long)]
    pub refresh: bool,
}

#[derive(Debug, Args, Default)]
pub(crate) struct ListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ValidateArgs {
    /// Pen name to validate.
    pub pen: String,
    /// Also validate workflow action `with:` blocks against upstream `action.yml`.
    /// Requires network for first-time ref resolution.
    #[arg(long)]
    pub check_actions: bool,
    /// Stop at the first failing file.
    #[arg(long)]
    pub fail_fast: bool,
    #[arg(long)]
    pub json: bool,
}

pub(crate) async fn run(args: SchemasArgs) -> Result<()> {
    match args.action {
        SchemasAction::Pull(a) => run_pull(a).await,
        SchemasAction::List(a) => run_list(&a),
        SchemasAction::Validate(a) => run_validate(a).await,
    }
}

pub(crate) async fn run_pull(args: PullArgs) -> Result<()> {
    let cfg = load_default()?;
    run_pull_with_config(&cfg, args.refresh).await
}

/// Entry point reusable by `nave init`. Never panics on network failure;
/// logs a warning and returns Ok so init can complete.
pub(crate) async fn run_pull_with_config(cfg: &NaveConfig, refresh: bool) -> Result<()> {
    let root = cache_root()?;
    let needed = schemas_for_tracked(&cfg.scan.tracked_paths);
    if needed.is_empty() {
        info!("no tracked paths require schemas; nothing to pull");
        return Ok(());
    }
    let ids: Vec<SchemaId> = needed.into_iter().collect();
    let reg = SchemaRegistry::new(root, cfg.schemas.clone())?;
    let result = if refresh {
        reg.refresh_all().await
    } else {
        reg.ensure_cached(&ids).await
    };
    if let Err(e) = result {
        warn!(error = %e, "schema pull failed");
        return Err(e);
    }
    info!(count = ids.len(), "schemas ready");
    Ok(())
}

fn run_list(args: &ListArgs) -> Result<()> {
    #[derive(serde::Serialize)]
    struct Row {
        id: &'static str,
        cached: bool,
        path: String,
        size_bytes: Option<u64>,
        source_url: Option<String>,
    }

    let cfg = load_default()?;
    let root = cache_root()?;
    let reg = SchemaRegistry::new(&root, cfg.schemas.clone())?;

    let rows: Vec<Row> = SchemaId::all()
        .iter()
        .map(|id| {
            let path = reg.schema_path(*id);
            let (cached, size) = match std::fs::metadata(&path) {
                Ok(m) => (true, Some(m.len())),
                Err(_) => (false, None),
            };
            Row {
                id: id.as_str(),
                cached,
                path: path.display().to_string(),
                size_bytes: size,
                source_url: cfg.schemas.sources.get(id.as_str()).cloned(),
            }
        })
        .collect();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        for r in &rows {
            let mark = if r.cached { "✓" } else { "·" };
            let size = r
                .size_bytes
                .map(|b| format!(" ({b} B)"))
                .unwrap_or_default();
            println!("{mark} {:<20} {}{}", r.id, r.path, size);
        }
    }
    Ok(())
}

#[derive(serde::Serialize, Clone)]
struct FileOutcome {
    owner: String,
    repo: String,
    path: String,
    schema: Option<&'static str>,
    schema_errors: Vec<String>,
    action_errors: Vec<String>,
}

async fn run_validate(args: ValidateArgs) -> Result<()> {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use nave_pen::{load_pen, resolve_pen_root, tracked_files_in_pen};

    let cfg = load_default()?;
    let pen_root_path = resolve_pen_root(&cfg.pen)?;
    let pen = load_pen(&pen_root_path, &args.pen)?;

    let cache = cache_root()?;
    let registry = SchemaRegistry::new(&cache, cfg.schemas.clone())?;

    // Make sure every schema we might need is cached up-front.
    let needed = [
        SchemaId::Dependabot,
        SchemaId::GithubWorkflow,
        SchemaId::GithubAction,
        SchemaId::Pyproject,
    ];
    registry.ensure_cached(&needed).await?;

    let http = reqwest::Client::builder()
        .user_agent(concat!("nave/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let files = tracked_files_in_pen(&pen_root_path, &pen, &cfg.scan)?;

    // Group files by (owner, repo) — each group becomes one progress bar.
    let mut groups: BTreeMap<(String, String), Vec<nave_pen::TrackedFile>> = BTreeMap::new();
    for tf in files {
        groups
            .entry((tf.owner.clone(), tf.repo.clone()))
            .or_default()
            .push(tf);
    }

    let registry = Arc::new(registry);
    let http = Arc::new(http);
    let cache = Arc::new(cache);

    let label_width = groups
        .keys()
        .map(|(o, r)| o.len() + 1 + r.len())
        .max()
        .unwrap_or(0);

    let outcomes: Vec<FileOutcome> = if args.json {
        run_validate_quiet(
            &groups,
            &registry,
            &http,
            &cache,
            args.check_actions,
            args.fail_fast,
        )
        .await?
    } else {
        run_validate_with_bars(
            groups,
            registry.clone(),
            http.clone(),
            cache.clone(),
            args.check_actions,
            args.fail_fast,
            label_width,
        )
        .await?
    };

    let failures = outcomes
        .iter()
        .filter(|o| !o.schema_errors.is_empty() || !o.action_errors.is_empty())
        .count();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&outcomes)?);
    } else {
        let any_failed = failures > 0;
        if any_failed {
            println!();
            println!("── failures ──");
            for o in &outcomes {
                if o.schema_errors.is_empty() && o.action_errors.is_empty() {
                    continue;
                }
                println!(
                    "✗ {}/{} :: {}  [{}]",
                    o.owner,
                    o.repo,
                    o.path,
                    o.schema.unwrap_or("-"),
                );
                for e in &o.schema_errors {
                    println!("    schema: {e}");
                }
                for e in &o.action_errors {
                    println!("    action: {e}");
                }
            }
        }
        println!("\n{} files, {} failed", outcomes.len(), failures);
    }

    if failures > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// JSON / quiet path: straightforward sequential validation, no bars.
async fn run_validate_quiet(
    groups: &std::collections::BTreeMap<(String, String), Vec<nave_pen::TrackedFile>>,
    registry: &std::sync::Arc<SchemaRegistry>,
    http: &reqwest::Client,
    cache: &std::path::Path,
    check_actions: bool,
    fail_fast: bool,
) -> Result<Vec<FileOutcome>> {
    let mut outcomes = Vec::new();
    for files in groups.values() {
        for tf in files {
            let outcome = validate_one(tf, registry, http, cache, check_actions).await;
            let failed = !outcome.schema_errors.is_empty() || !outcome.action_errors.is_empty();
            outcomes.push(outcome);
            if failed && fail_fast {
                return Ok(outcomes);
            }
        }
    }
    Ok(outcomes)
}

/// Interactive path: one `indicatif` bar per repo, all coordinated through `MultiProgress`.
/// Repos run concurrently as async tasks; each bar ticks steadily so animation is smooth.
async fn run_validate_with_bars(
    groups: std::collections::BTreeMap<(String, String), Vec<nave_pen::TrackedFile>>,
    registry: std::sync::Arc<SchemaRegistry>,
    http: std::sync::Arc<reqwest::Client>,
    cache: std::sync::Arc<std::path::PathBuf>,
    check_actions: bool,
    fail_fast: bool,
    label_width: usize,
) -> Result<Vec<FileOutcome>> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

    let mp = MultiProgress::new();

    let style =
        ProgressStyle::with_template("{prefix:.bold} [{bar:30.cyan/blue}] {pos}/{len} {wide_msg}")
            .unwrap()
            .progress_chars("=>-");

    let finished_style = ProgressStyle::with_template("{prefix} {msg}").unwrap();

    // Build every bar up front.
    let mut work: Vec<((String, String), Vec<nave_pen::TrackedFile>, ProgressBar)> =
        Vec::with_capacity(groups.len());
    for ((owner, repo), files) in groups {
        let pb = mp.add(ProgressBar::new(files.len() as u64));
        pb.set_style(style.clone());
        let label = format!("{owner}/{repo}");
        pb.set_prefix(format!("{label:<label_width$}"));
        // Steady tick is what actually makes the MultiProgress animate.
        pb.enable_steady_tick(Duration::from_millis(50));
        work.push(((owner, repo), files, pb));
    }

    let stop = Arc::new(AtomicBool::new(false));

    // Spawn one async task per repo. Concurrency without threads.
    let mut tasks = Vec::with_capacity(work.len());
    for ((owner, repo), files, pb) in work {
        let registry = registry.clone();
        let http = http.clone();
        let cache = cache.clone();
        let stop = stop.clone();
        let finished_style = finished_style.clone();

        let handle = tokio::spawn(async move {
            let mut out: Vec<FileOutcome> = Vec::with_capacity(files.len());
            let mut bad = 0usize;

            for tf in &files {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                pb.set_message(tf.relpath.clone());

                let outcome = validate_one(tf, &registry, &http, &cache, check_actions).await;

                let failed = !outcome.schema_errors.is_empty() || !outcome.action_errors.is_empty();
                if failed {
                    bad += 1;
                    if fail_fast {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
                out.push(outcome);
                pb.inc(1);

                // Yield so other repo-tasks get a turn and the bar paints.
                tokio::task::yield_now().await;
            }

            pb.set_style(finished_style);
            if bad == 0 {
                pb.finish_with_message(format!("✓ {owner}/{repo}"));
            } else {
                pb.finish_with_message(format!("✗ {owner}/{repo}  ({bad} failed)"));
            }
            out
        });
        tasks.push(handle);
    }

    let mut outcomes = Vec::new();
    for t in tasks {
        outcomes.extend(t.await?);
    }

    drop(mp);
    Ok(outcomes)
}

async fn validate_one(
    tf: &nave_pen::TrackedFile,
    registry: &std::sync::Arc<SchemaRegistry>,
    http: &reqwest::Client,
    cache: &std::path::Path,
    check_actions: bool,
) -> FileOutcome {
    use nave_parse::{parse_file, to_json};
    use nave_schemas::schema_for_path;

    let schema = schema_for_path(&tf.relpath);
    let mut schema_errors: Vec<String> = Vec::new();
    let mut action_errors: Vec<String> = Vec::new();

    if let Some(id) = schema {
        match parse_file(&tf.abspath) {
            Ok(doc) => match to_json(&doc) {
                Ok(instance) => {
                    let result = registry.validate(id, &instance);
                    match result {
                        Ok(errs) => schema_errors = errs,
                        Err(e) => schema_errors.push(format!("validator error: {e}")),
                    }
                }
                Err(e) => schema_errors.push(format!("json conversion: {e}")),
            },
            Err(e) => schema_errors.push(format!("parse: {e}")),
        }

        if check_actions
            && matches!(id, SchemaId::GithubWorkflow)
            && let Err(e) =
                validate_workflow_actions(http, cache, &tf.abspath, &mut action_errors).await
        {
            action_errors.push(format!("action check failed: {e}"));
        }
    }

    FileOutcome {
        owner: tf.owner.clone(),
        repo: tf.repo.clone(),
        path: tf.relpath.clone(),
        schema: schema.map(|s| s.as_str()),
        schema_errors,
        action_errors,
    }
}

async fn validate_workflow_actions(
    http: &reqwest::Client,
    cache_root: &std::path::Path,
    workflow_path: &std::path::Path,
    out: &mut Vec<String>,
) -> Result<()> {
    use nave_parse::{Format, parse_bytes};
    use nave_schemas::{ActionRef, check_with_block, fetch_action};
    use serde_json::Value;

    let bytes = std::fs::read(workflow_path)?;
    let doc = parse_bytes(&bytes, Format::Yaml)?;
    let v = nave_parse::to_json(&doc)?;

    let Some(jobs) = v.get("jobs").and_then(|j| j.as_object()) else {
        return Ok(());
    };

    for (job_name, job) in jobs {
        let Some(steps) = job.get("steps").and_then(|s| s.as_array()) else {
            continue;
        };
        for (i, step) in steps.iter().enumerate() {
            let Some(uses) = step.get("uses").and_then(|u| u.as_str()) else {
                continue;
            };
            let Some((owner_repo, git_ref)) = uses.split_once('@') else {
                continue;
            };
            // Skip subpath actions: owner/repo/path@ref — not supported here.
            let slashes = owner_repo.matches('/').count();
            if slashes != 1 {
                continue;
            }
            let (owner, repo) = owner_repo.split_once('/').unwrap();

            let fetched = match fetch_action(
                http,
                cache_root,
                ActionRef {
                    owner,
                    repo,
                    user_ref: git_ref,
                },
            )
            .await
            {
                Ok(f) => f,
                Err(e) => {
                    out.push(format!(
                        "jobs.{job_name}.steps[{i}] uses={uses}: fetch failed: {e}"
                    ));
                    continue;
                }
            };

            let empty = Value::Object(serde_json::Map::new());
            let provided = step.get("with").unwrap_or(&empty);
            match check_with_block(&fetched.manifest, provided) {
                Ok(check) => {
                    for k in &check.missing_required {
                        out.push(format!(
                            "jobs.{job_name}.steps[{i}] uses={uses}: missing required input `{k}`"
                        ));
                    }
                    for k in &check.unknown {
                        out.push(format!(
                            "jobs.{job_name}.steps[{i}] uses={uses}: unknown input `{k}`"
                        ));
                    }
                    for (k, msg) in &check.deprecated_used {
                        out.push(format!(
                            "jobs.{job_name}.steps[{i}] uses={uses}: deprecated input `{k}`: {msg}"
                        ));
                    }
                }
                Err(e) => {
                    out.push(format!(
                        "jobs.{job_name}.steps[{i}] uses={uses}: check failed: {e}"
                    ));
                }
            }
        }
    }
    Ok(())
}
