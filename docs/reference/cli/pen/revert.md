# `nave pen revert`

Drop local commits on the pen branch, returning to the synced baseline.

## Usage

```
Drop local commits on the pen branch, returning to the synced baseline

Usage: nave pen revert [OPTIONS] <NAME>

Arguments:
  <NAME>  Pen name

Options:
      --allow-dirty  Discard uncommitted working-tree changes before
                     proceeding. Without this, dirty repos cause the
                     command to abort.
  -h, --help         Print help
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
