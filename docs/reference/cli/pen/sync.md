# `nave pen sync`

Refresh a pen's synced baseline against the fleet cache.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen/sync.txt"
```

## What it does

1. Re-evaluates the pen's filter against the current cache.
2. Compares the result set to the pen's recorded repo list.
3. For each repo still in both: fast-forward its sync baseline to the cache's
   current HEAD.
4. Reports additions, removals, and freshenings.

This is what takes a pen from `stale` → `fresh`.

## Freshness and the filter contract

A pen is an assertion: "at create time, these repos matched the filter." Sync checks
whether that assertion still holds. Ways it can go stale:

- A new repo now matches the filter (e.g. a config file was added).
- An existing repo no longer matches (e.g. a config file was removed or modified).
- A repo still matches but its default branch has advanced.

`--dry-run` shows the diff without applying.

## Not a force-push

Sync updates the pen's *baseline* — the reference point it considers "the synced
state". It does not overwrite pen branches or discard local work. For that, see
[`reinit`](reinit.md).

## Prune

Currently, sync doesn't delete repos that have left the filter (this is an open
design question). The planned behaviour is a `--prune` flag, matching the rest of
the CLI's conventions.
