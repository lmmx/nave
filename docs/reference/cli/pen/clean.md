# `nave pen clean`

Discard uncommitted working-tree changes across a pen's repos.

## Usage

```
Discard uncommitted working-tree changes across a pen's repos

Usage: nave pen clean <NAME>

Arguments:
  <NAME>  Pen name

Options:
  -h, --help  Print help
```

## What it does

For each repo in the pen:

1. `git reset --hard HEAD` to discard staged and unstaged changes.
2. `git clean -fd` to remove untracked files.

This takes every repo back to its current commit — whichever that is (pen branch tip,
possibly including run-local commits).

## Not a reset to baseline

`clean` removes uncommitted changes. It does **not** roll back committed work on the
pen branch. For that, use [`revert`](revert.md) (go to synced baseline) or
[`reinit`](reinit.md) (go to origin default branch).

## Irreversible

As with any `git clean -fd`, this cannot be undone from the working tree alone. If
you might want the changes later, commit them first.
