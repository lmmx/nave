# ++"nave pull"++

Sparse-checkout scanned repositories into the local cache.

## Usage

```bash
--8<-- "docs/_snippets/cli/pull.txt"
```

## What it does

For each repo in the scan index:

1. If not cloned: shallow clone with a sparse-checkout cone limited to tracked paths.
2. If cloned and in sync (local HEAD matches scan index HEAD): skip.
3. If cloned but stale: fast-forward or re-fetch as appropriate.
4. If cloned but diverged: re-clone (rare; occurs when the remote's default branch
   has been force-pushed).

## Reports

On completion, pull logs:

- `cloned` — new sparse clones created
- `updated` — existing clones advanced to match scan index
- `recloned` — clones blown away and rebuilt
- `skipped` — already up-to-date, no work
- `failed` — clone or fetch failed (network, auth, deleted repo)
- `sha_mismatches` — local HEAD disagrees with what scan recorded (usually transient)

## Design notes

- Only tracked files are fetched (sparse checkout cone from `tracked_paths`).
- Shallow by depth=1: no history, only the latest revision.
- No analysis, no parsing, no validation — those are the analysis layer's job.
- Idempotent: re-running produces no work if nothing has changed.

See [Cache](../../concepts/cache.md) for the broader context.
