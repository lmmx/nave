# Cache

**TL;DR:** The cache is a local sparse-checkout projection of the fleet's tracked files,
plus a metadata index. It's the input to every analysis command, and it's rebuilt
incrementally from `pushed_at` timestamps.

## Two things in one

The cache has two components living under the same root (`~/.cache/nave/` by default):

1. **Scan index** — `meta.toml`, recording per-repo metadata: `pushed_at`, default branch,
   tracked file paths, HEAD SHA at time of scan.
2. **Sparse checkouts** — one directory per repo, containing only tracked files.

The scan index is populated by `nave scan`. The sparse checkouts are populated by
`nave pull`, using the index to decide what to fetch.

## Why sparse checkout

A full clone of every repo in a 50-repo fleet is wasteful: Nave only reads a handful
of config files from each. Sparse checkout lets git fetch just the files matching the
tracked paths, leaving the rest of the tree unrealised on disk. For most fleets this
is a ~100× space saving.

The sparse-checkout cone is derived from `tracked_paths` in config.

## Incrementality

By default, `nave scan` only re-examines repos whose `pushed_at` is newer than the most
recent scan. This makes repeated scans cheap — seconds rather than minutes.

To force a full rescan (e.g. after narrowing `tracked_paths` to drop repos that no
longer match):

```bash
rm ~/.cache/nave/meta.toml
nave scan --prune
```

`--prune` removes cached repo directories that no longer match the current filters.

## Eventual consistency

The cache is a materialised projection of a live remote. It's *eventually consistent*
with the fleet: at any given moment, the upstream may have moved. This is why pen
operations have an explicit freshness contract — see [Pens](pens.md#freshness).

For read-only analysis, staleness is mostly fine (and fast). For writes, it's a
correctness issue: running a codemod against a stale cache could silently skip repos
that would now match the filter, or operate on obsolete config.

## What never lives in the cache

- File history — only the current revision of each tracked file is materialised.
- Non-tracked files — anything outside the sparse-checkout cone.
- Transaction state — pen workspaces are kept completely separate (`~/.local/share/nave/pens/`)
  so that cache writes and pen writes can never collide.
