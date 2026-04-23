# `nave pen exec`

Run a command in each pen repo, optionally committing/pushing changes.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen/exec.txt"
```

## What it does

For each repo in the pen (or just `--only` if given):

1. `cd` into the repo's pen workspace.
2. Run the command (inherited stdout/stderr/env).
3. If `--commit` or `--push-changes`: stage and commit any modifications.
4. If `--push-changes`: push the pen branch to `origin`.

## Examples

```bash
# Run a python edit script in each pen repo
nave pen exec nave/lowest-direct -- python /path/to/edit.py

# Commit and push the result
nave pen exec nave/lowest-direct --push-changes \
  -m "ci: add --resolution lowest-direct" -- \
  python /path/to/edit.py

# Single-repo execution
nave pen exec nave/lowest-direct --only lmmx/comrak -- \
  ruff check --fix .

# Ad-hoc: show git status in each repo
nave pen exec nave/lowest-direct -- git status -s
```

## Effect on run state

- Without `--commit`: repo is left dirty; run state is unaffected.
- With `--commit`: run state → `run-local`.
- With `--push-changes`: run state → `run-pushed`.

## Interaction with the planned `run`

`exec` is the general-purpose escape hatch. The planned `nave pen run` will be a
structured alternative: declarative codemod specifications with schema validation,
rather than arbitrary commands. `exec` will remain available.
