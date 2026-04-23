# `nave pen`

Operations on pens — named subsets of the fleet.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen.txt"
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
