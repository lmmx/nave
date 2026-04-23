# `nave pen`

Operations on pens — named subsets of the fleet.

## Usage

```
Operations on pens (named subsets of the fleet)

Usage: nave pen <COMMAND>

Commands:
  create  Create a pen by filtering the fleet and cloning matching repos
  list    List pens, optionally filtered by state
  show    Show a single pen's details
  status  Show per-repo state for a pen: working tree, freshness,
          run state, divergence
  sync    Refresh a pen's synced baseline against the fleet cache
  clean   Discard uncommitted working-tree changes across a pen's repos
  revert  Drop local commits on the pen branch, returning to the synced baseline
  reinit  Rebuild the pen branch from origin's default branch
  exec    Run a command in each pen repo, optionally committing/pushing changes
  rm      Remove a pen's local workspace and definition
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

See [Pens](../../concepts/pens.md) for the concept.

## Planned (🚧 not yet shipped)

The following are designed and described in the
[orchestration essay](https://cog.spin.systems/fleet-ops-orchestrating-codemods)
but not yet available in the CLI:

- `nave pen run` — apply a declarative codemod + push branches.
- `nave pen open` — create PRs (wrapping `gh pr create`).
- `nave pen merge` — merge PRs (wrapping `gh pr merge`).
- `nave pen close` — close open pen PRs.
- `nave pen prune` — remove pens that have run but reference deleted remotes.

In the meantime: use `nave pen exec` for arbitrary per-repo commands, and
drive PRs manually with `gh`.

## Subcommand pages

- [`create`](pen/create.md)
- [`list`](pen/list.md)
- [`show`](pen/show.md)
- [`status`](pen/status.md)
- [`sync`](pen/sync.md)
- [`clean`](pen/clean.md)
- [`revert`](pen/revert.md)
- [`reinit`](pen/reinit.md)
- [`exec`](pen/exec.md)
- [`rm`](pen/rm.md)
