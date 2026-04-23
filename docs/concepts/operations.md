# Operations

**TL;DR:** Nave's commands fall into three layers — **read**, **validate**, **write**.
Only the write layer (pens) mutates the fleet. Each layer is a composition of lower
ones.

## The three layers

### Read layer

| Command  | What it produces                                 |
|----------|--------------------------------------------------|
| `scan`   | Repo index in the cache                          |
| `pull`   | Tracked files, sparse-checked-out                |
| `search` | Repos/files/hole-addresses matching a query      |
| `build`  | Templates + hole reports for a file kind         |

These never write to the fleet. They may update the local cache, but that's a local
projection, not a remote effect.

### Validate layer

| Command              | What it produces                                |
|----------------------|-------------------------------------------------|
| `check`              | Parse/round-trip outcomes per tracked file      |
| `schemas validate`   | Per-file schema errors; per-step action errors  |

Validation is derived from the cache — it doesn't touch the fleet. It's *preparation*
for a write: a failing `check` or `validate` is a signal that a bulk edit will land
on shaky ground.

### Write layer

| Command subtree | What it does                                    |
|-----------------|-------------------------------------------------|
| `pen`           | Clone subsets, run codemods, push branches, (eventually) open PRs |

Pens are the only thing in Nave that can mutate remote state, and every write goes
through the pen lifecycle. There is no "just push this one commit" shortcut.

## Composition

The layers compose strictly upward:

```
write (pen) ──► reads (search, build) ──► cache (scan, pull) ──► fleet
```

A pen uses `search` internally to select repos; `search` reads the cache; the cache
was built by `scan` and `pull`. You can run any layer standalone, but you can't skip
a lower one.

## Safety consequences

The read/validate/write split has two safety properties worth making explicit:

- **Local analysis is free.** `search`, `build`, and `schemas validate` have no
  network dependency after the cache is populated. Iterating a codemod design is
  fast and cheap because you're not round-tripping to GitHub.
- **Writes are audited by construction.** Every pen write leaves a trail: a pen
  workspace on disk, a branch on the remote, eventually a PR. You can revert or
  clean up at any stage without touching the main branch.

## Failure modes

Each layer has its own failure mode, and the layer's reaction is different:

- **Read failure** (e.g. 404 on scan) — the failing repo is skipped, the rest proceed.
- **Validate failure** — the file is flagged; aggregate report exits non-zero.
- **Write failure** — the pen enters a half-run state; `pen status` surfaces it;
  `pen revert` rolls back.

You can think of this as the error budget being highest at the top (writes) and
lowest at the bottom (reads that are effectively cached queries).
