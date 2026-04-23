# Pens

A pen is a named, ephemeral (meaning disposable) yet durable transaction workspace
containing full shallow clones (all files in the repo, to a history depth of 1)
of a filtered subset of the fleet.

Pens are where codemods run, so they are always on a non-default branch (which matches
the pen's name).

Everything else in Nave is read-only; pens are the only writer.

## What a pen is

A pen is the combination of:

- A **filter over repos** — its selection rule (same query syntax as [++"nave search"++](../reference/cli/search.md)).
- A **set of intended transformations** — the codemod.
- A **contract** — applicability conditions, primarily freshness of the cache
  against the remote.

Pens live at `~/.local/share/nave/pens/<pen-name>/`, in the user-local application data directory
(`$XDG_DATA_HOME`).

They don't live in `/tmp`: pens are ephemeral in intent but durable for the duration of a transaction,
so they need to survive reboots and be crash-safe mid-operation.

## Why full clones instead of cache reuse

The cache exists to mirror the default branch. Dirtying it with draft changes would
couple two concerns that have different lifecycles: the cache is an eventually
consistent read projection, the pen is an isolated write transaction.

So pens clone fresh, shallowly but not sparsely. "Not sparse" matters because
code edits sometimes need to touch files outside the tracked set,
such as updating a lockfile after a dependency bump.

A shallow-but-full clone keeps pen storage size on disk proportional to the number of repos,
not the fleet size × history.

## Lifecycle

```
create ──► sync ──► exec/run ──► (open) ──► (merge) ──► rm
            ▲                         │
            └── freshen ──────────────┘
```

1. **create** — clone matching repos, create pen branches, record the filter.
2. **sync** — re-evaluate the filter against a fresh scan; note drift.
3. **exec** — run arbitrary commands in each repo. Does not push.
4. **run** — apply the codemod and push branches. 🚧 *Codemod authoring is still under design.*
5. **open / merge / close** — PR lifecycle. 🚧 *PR integration is planned but not yet shipped.*
6. **rm** — remove local state. `--purge` also removes remote branches.

See [++"nave pen"++](../reference/cli/pen.md) for the full command list.

## States

Every pen repo has three orthogonal state axes:

| Axis          | Values                                     | Source                         |
|---------------|--------------------------------------------|--------------------------------|
| Working tree  | clean / dirty / missing                    | git status                     |
| Freshness     | fresh / stale                              | vs. current cache              |
| Run state     | not-run / run-local / run-pushed           | pen's own run ledger           |
| Divergence    | up-to-date / ahead / behind / diverged     | vs. origin/pen-branch          |

[++"nave pen status"++](../reference/cli/pen/status.md) surfaces all four.
[++"nave pen list"++](../reference/cli/pen/list.md) summarises counts.

These states are operational gates:

- [++"nave pen run"++](../reference/cli/pen/run.md) refuses to proceed on a stale pen (use `--freshen` to sync first).
- [++"nave pen run"++](../reference/cli/pen/run.md) also refuses on a dirty tree (use `--allow-dirty`).
- [++"nave pen revert"++](../reference/cli/pen/revert.md) / [++"nave pen reinit"++](../reference/cli/pen/reinit.md) refuse on a dirty tree (use `--allow-dirty`).

The friction is deliberate: a codemod applied to a stale filter silently skips repos
that would now match; a codemod applied over uncommitted garbage silently includes it.

## Freshness

A pen's freshness contract says: the set of repos this pen selected at `create` time
is still the set that matches the filter *now*. Two ways a pen can go stale:

- **Repos entering the pen** — a repo is newly created or modified such that it now
  meets the filter (e.g. a config file is added).
- **Repos leaving the pen** — a repo's pen branch gets merged, the default branch
  changes, and the filter no longer matches.

[++"nave pen sync"++](../reference/cli/pen/sync.md) reconciles this.
`--dry-run` reports without touching anything.
`--prune` removes repos that no longer match.

## Naming

Pens have auto-generated names of the form `nave/<slug>[-<n>]`, derived from the
filter's first term (`workflow:a-b` → `nave/a-b`). The name doubles as the branch
name on each repo, so it has to be:

- Slug-safe (lower-case, alphanumeric, hyphens).
- Unique across the local filesystem.
- Unique across each target repo's refs (checked at create time).

You can override with `--name`, but the `nave/` prefix is retained by default to
avoid clashes with other tooling.

## Why "pen"

The pet metaphor is exhausted in infrastructure lit ("pets vs. cattle"). Pens hold
the cattle while you work on them, then let them back out into the fleet. They're
scoped working sets, not deployment targets.
