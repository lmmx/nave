# Lifecycle overview

**TL;DR:** Nave follows a five-stage pipeline — init, scan, pull, analyse, operate —
with a clean split between data (scan/pull), analysis (search/build/check/schemas),
and mutation (pen).

## The five stages

### 1. init

`nave init` creates `~/.config/nave.toml` (interactively by default). It probes
`gh` for your username, writes a commented config with defaults, and pulls the
schema cache.

Run this once.

### 2. scan

`nave scan` enumerates the user's public repos via the GitHub API and indexes which
tracked files exist in each. It writes a metadata index to the cache — but no file
bodies.

Scan is incremental by default: repos are only re-examined if their `pushed_at` is
newer than the last scan.

### 3. pull

`nave pull` performs sparse-checkout shallow clones of scanned repos into the cache,
materialising only the tracked files. This is what gives you a local working copy of
the fleet.

Pull uses the scan index, so `scan` must have run first.

### 4. analyse

Four commands derive from the cache:

- `nave search` — substring and structural queries across tracked files.
- `nave build` — anti-unified templates showing shared skeleton + holes.
- `nave check` — verify configs parse and round-trip cleanly.
- `nave schemas validate` — check files against JSON Schemas and action inputs.

None of these write to the fleet. All are fast because they're local.

### 5. operate

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

The five stages group into three phases:

- **Data** (scan, pull) — populate the cache.
- **Analysis** (search, build, check, schemas) — derive insight from the cache.
- **Mutation** (pen) — create isolated transactions and push changes back.

Every command belongs to exactly one phase. The phases compose upward only: you
can't mutate without analysing (the filter is a query), you can't analyse without
data (the cache must exist). See [Invariants](invariants.md) for the strict rules.
