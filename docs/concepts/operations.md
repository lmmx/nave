# Operations

Nave's commands fall into three phases — **read** (fleet), **validate** (schemas), and **write**
(pens).

Only the write phase (pens) mutates the fleet.

### Fleet reading

| Command  | What it produces                                 |
|----------|--------------------------------------------------|
| `scan`   | Repo index in the cache                          |
| `pull`   | Tracked files, sparse-checked-out                |
| `search` | Repos/files/hole-addresses matching a query      |
| `build`  | Templates + hole reports for a file kind         |

They may update the local cache as a local projection, not a remote effect.

### Schema validation

| Command              | What it produces                                |
|----------------------|-------------------------------------------------|
| `check`              | Parse/round-trip outcomes per tracked file      |
| `schemas validate`   | Per-file schema errors; per-step action errors  |

Validation is derived from the cache, and still doesn't have any effect on the fleet.
It's part of preparation for write operations: a failing `check` or `validate` is a signal
that a bulk edit will land on shaky ground.

### Pen writing

| Command subtree | What it does                                    |
|-----------------|-------------------------------------------------|
| `pen create / sync / clean / revert / reinit / rm` | Pen lifecycle |
| `pen rewrite`   | Apply declarative transforms to working trees       |
| `pen exec`      | Run an arbitrary command per repo                   |
| `pen run`       | (Planned) Rewrite + commit + push                   |

Pens are the only thing in Nave that can mutate remote state, and every write goes
through the pen lifecycle. There is no "just push this one commit" shortcut.

## Composition

Naturally, the phases build on one another:

```
write (pen) ──► reads (search, build) ──► cache (scan, pull) ──► fleet
```

A pen uses `search` internally to select repos; `search` reads the cache; the cache
was built by `scan` and `pull`. You can run any phase standalone, but you can't skip
a lower one.

## Safety consequences

The read/validate/write split has two safety properties:

- **Local analysis is free.** `search`, `build`, and `schemas validate` have no
  network dependency after the cache is populated. Iterating a codemod design is
  fast and cheap because you're not round-tripping to GitHub.
- **Writes are audited by construction.** Every pen write leaves a trail: first a
  pen workspace on disk (on a branch whose name matches the pen name), then as a
  branch on the remote (pushed on [++"nave pen run"++](../reference/cli/pen/run.md)),
  and eventually a PR (on [++"nave pen open"++](../reference/cli/pen/open.md)),
  merged with [++"nave pen merge"++](../reference/cli/pen/merge.md).
  You can revert or clean up at any stage without touching your default branch.

## Failure modes

Each phase is conscious of the potential issues and care is taken so the fleet moves
without leaving any individual repo in an inconsistent state.

Several commands will halt if the workspace has untracked/uncommitted files (what git calls
a "dirty" state), and repos are checked for freshness by [++"nave scan"++](../reference/cli/scan.md) to avoid commits
being rejected upon push (where the local git state was not updated to reflect the remote's).

Additionally, since editing config files can produce invalid config,
schema validation is performed wherever possible (including for GitHub Actions
based on the declared inputs from the remote `action.yml`).

There is a "four dimensional" model of the state of each repo that can be tracked using the
`status`/`show`/`list` commands:

1. Working tree cleanliness (vs. the git state), which can be ensured by [++"nave pen clean"++](../reference/cli/pen/clean.md)
2. Local repo freshness (vs. the remote), which can be ensured by [++"nave pen revert"++](../reference/cli/pen/revert.md)
3. Completion of a rewrite operation (committing and pushing to the remote) from [++"nave pen run"++](../reference/cli/pen/run.md)
4. PR state, modified upon [++"nave pen open"++](../reference/cli/pen/open.md), [++"nave pen merge"++](../reference/cli/pen/merge.md)
   and [++"nave pen close"++](../reference/cli/pen/close.md)
