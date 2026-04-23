# Cache

The cache is a local sparse-checkout projection of the fleet's tracked files,
with a metadata index. It's the input to every analysis command, and is rebuilt
incrementally from `pushed_at` timestamps.

The cache has two components living under the same root (`~/.cache/nave/` by default):

1. **Scan index** populated by `nave scan`: `meta.toml`, recording per-repo metadata: `pushed_at`, default branch,
   tracked file paths, HEAD SHA at time of scan (to check the "freshness" of repos).
2. **Sparse checkouts** populated by `nave pull`: one directory per repo, containing only tracked
   files. It uses the index to decide what to fetch (only new or stale repos).

## Why a sparse checkout?

A full clone of every repo in a fleet of many repos would be slower and wasteful of disk space.

Nave only reads a handful of "tracked" config files from each. Which files are tracked (and thus in
the sparse-checkout cone) is set from the `tracked_paths` in [config](config.md).

Sparse checkout lets git fetch just the files matching the tracked paths,
leaving the rest of the tree unrealised on disk.

For most fleets this is a ~100× space saving.

## Incrementality

`nave scan` only re-examines repos whose `pushed_at` is newer than the most recent scan,
so repeated scans are instant if none of the remote repos in the user's fleet were pushed to
in the interim.

To perform a full re-scan (e.g. after narrowing `tracked_paths` to drop repos that no longer match):

```bash
rm ~/.cache/nave/meta.toml
nave scan --prune
```

`--prune` removes cached repo directories that no longer match the current filters.

## Eventual consistency

The cache is a materialised projection of a live remote. It's *eventually consistent*
with the fleet: at any given moment, the upstream may have moved. This is why pen
operations have an explicit freshness contract — see [Pens](pens.md#freshness).

For read-only analysis, staleness is mostly fine (and fast).

For writes, it's an issue of correctness: running a codemod against a stale cache could silently skip repos
that would now match the filter, or operate on obsolete config (which might give you merge conflicts upon PR).

## The cache does not store history or make changes

To keep clones small and fast, only the current revision of each tracked file is materialised.

Anything outside the sparse-checkout cone will not be in there.

Making changes to repos is a different matter. Edits are considered transaction state,
which are kept in workspaces called "pens", completely separate (`~/.local/share/nave/pens/`).

Cache writes and pen writes can never collide, and most simply the cache is always on the default
branch, while pens are always on a non-default branch (whose name is prefixed by `pen/`).
