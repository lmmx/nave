# ++"nave pen revert"++

Drop local commits on the pen branch, returning to the synced baseline.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen/revert.txt"
```

## What it does

For each repo: `git reset --hard <sync-baseline>`. The sync baseline is the commit
the pen was synced to (created or most recently synced from the cache).

Effect:

- Any commits made by `pen exec` or the planned `pen run` are dropped.
- The pen branch still exists; it now points at the baseline.
- Run state is reset to `not-run`.

## Dirty trees

By default, revert refuses to run if any repo has uncommitted changes (safety against
data loss). Pass `--allow-dirty` to discard them as part of the revert.

## Revert vs reinit

- `revert` — go back to the synced baseline (the state the pen was in after `create`
  or the last `sync`).
- `reinit` — go back to origin's default branch, recreating the pen branch from scratch.

Revert is the "undo local experimentation" operation; reinit is the
"start over completely" operation.
