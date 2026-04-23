# Lifecycle

Nave follows a pipeline beginning with `init`, `scan`, and `pull` to set up the fleet cache,
then analyse with `search`/`build`/`check`/`schemas`, and then operate on
subsets identified by the analysis step as a "pen" which has commands
to treat such sets of repos like Docker containers (`nave pen` + `create`/`run`/`rm`).

## Stages

### 1. `init` and `scan`

`nave init` creates `~/.config/nave.toml` (interactively by default). It probes
`gh` for your username, writes a commented config with defaults, and pulls the
schema cache.

You only have to run this once, and it'll be detected in future.

If `init` wasn't run yet, `nave scan` will run it first.

A scan enumerates the user's public repos via the GitHub API and indexes any of the
tracked file types in them. It writes a metadata index to the cache, but no repo files.

Scan is incremental by default: repos are only re-examined if their `pushed_at` is
newer than the last scan.

### 2. `pull`

`nave pull` puts a `checkout/` subdir in the cached fleet dirs that `nave scan` put
the repo metadata in (like last push time and their commit SHA).

A `pull` performs sparse-checkout shallow clones of scanned repos in those `checkout/` subdirs,
materialising only the tracked files, giving a local working copy of the fleet.

Pull uses the scan index, so `scan` must have run first.

### 3. Analyse

Four commands then derive from the fleet cache, allowing you to explore it before
deciding how to intervene (or just to monitor the situation):

- `nave search` — substring and structural queries across tracked files.
- `nave build` — anti-unified templates showing shared skeleton + holes.
- `nave check` — verify configs parse and round-trip cleanly.
- `nave schemas validate` — check files against JSON Schemas and CI Action inputs.

These are all read-only operations, and fast to run since they're using the local cache.

### 4. Rewrite

`nave pen` is the write layer. Pens create scoped workspaces, run codemods, push
branches, and (eventually) open and merge PRs. See [Pens](../concepts/pens.md).

## The typical flow

```
nave init                    # once
nave scan                    # often; incremental
nave pull                    # after scan
nave search / build / check  # as often as you like
nave pen create <filter>     # when you want to change something
nave pen run <pen>           # apply the codemod
nave pen rm <pen>            # clean up
```

## Data vs analysis vs mutation

The stages group into three phases:

- **Ingestion** (scan, pull) — populate the cache.
- **Analysis** (search, build, check, schemas) — derive insight from the cache.
- **Mutation** (pen) — create isolated transactions and push changes back.

Every command belongs to exactly one phase. The phases compose upward only: you
can't mutate without analysing (the filter is a query), you can't analyse without
data (the cache must exist). See [Invariants](invariants.md) for the strict rules.
