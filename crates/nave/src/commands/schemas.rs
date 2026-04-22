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

async fn run_validate(args: ValidateArgs) -> Result<()> {
    use nave_parse::{parse_file, to_json};
    use nave_pen::{load_pen, resolve_pen_root, tracked_files_in_pen};
    use nave_schemas::{SchemaId, SchemaRegistry, schema_for_path};

    let cfg = load_default()?;
    let pen_root_path = resolve_pen_root(&cfg.pen)?;
    let pen = load_pen(&pen_root_path, &args.pen)?;

    let cache = cache_root()?;
    let mut registry = SchemaRegistry::new(&cache, cfg.schemas.clone())?;

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

    #[derive(serde::Serialize)]
    struct FileOutcome {
        owner: String,
        repo: String,
        path: String,
        schema: Option<&'static str>,
        schema_errors: Vec<String>,
        action_errors: Vec<String>,
    }

    let mut outcomes = Vec::new();
    let mut failures = 0usize;

    for tf in &files {
        let schema = schema_for_path(&tf.relpath);
        let mut schema_errors: Vec<String> = Vec::new();
        let mut action_errors: Vec<String> = Vec::new();

        if let Some(id) = schema {
            match parse_file(&tf.abspath) {
                Ok(doc) => match to_json(&doc) {
                    Ok(instance) => match registry.validate(id, &instance) {
                        Ok(errs) => schema_errors = errs,
                        Err(e) => schema_errors.push(format!("validator error: {e}")),
                    },
                    Err(e) => schema_errors.push(format!("json conversion: {e}")),
                },
                Err(e) => schema_errors.push(format!("parse: {e}")),
            }

            if args.check_actions && matches!(id, SchemaId::GithubWorkflow) {
                if let Err(e) =
                    validate_workflow_actions(&http, &cache, &tf.abspath, &mut action_errors).await
                {
                    action_errors.push(format!("action check failed: {e}"));
                }
            }
        }

        let failed = !schema_errors.is_empty() || !action_errors.is_empty();
        if failed {
            failures += 1;
        }

        outcomes.push(FileOutcome {
            owner: tf.owner.clone(),
            repo: tf.repo.clone(),
            path: tf.relpath.clone(),
            schema: schema.map(|s| s.as_str()),
            schema_errors,
            action_errors,
        });

        if failed && args.fail_fast {
            break;
        }
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&outcomes)?);
    } else {
        for o in &outcomes {
            let mark = if o.schema_errors.is_empty() && o.action_errors.is_empty() {
                "✓"
            } else {
                "✗"
            };
            let schema = o.schema.unwrap_or("-");
            println!("{mark} {:<40} [{}]  {}/{}", o.path, schema, o.owner, o.repo);
            for e in &o.schema_errors {
                println!("    schema: {e}");
            }
            for e in &o.action_errors {
                println!("    action: {e}");
            }
        }
        println!("\n{} files, {} failed", outcomes.len(), failures,);
    }

    if failures > 0 {
        std::process::exit(1);
    }
    Ok(())
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
