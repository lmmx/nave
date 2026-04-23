# State model

Nave has five explicit states — Fleet, Scan, Cache, Analysis, Pen — and strict
rules about which operations are allowed to touch which state. Reads never write. Only
pens mutate.

## The five states

| State     | What it is                                      | Scope         |
|-----------|-------------------------------------------------|---------------|
| Fleet     | The canonical set of repos on GitHub            | Remote        |
| Scan      | A local index of repo metadata (no file bodies) | Local cache   |
| Cache     | Sparse-checkout working trees of tracked files  | Local cache   |
| Analysis  | Derived outputs (search hits, templates, holes) | In-memory     |
| Pen       | A mutable transaction workspace                 | Local, per-op |

## The flow

```
fleet ──► scan ──► cache ──► analysis
                                │
                                ▼
                               pen ──► fleet
```

`scan` and `pull` produce data. `search`, `build`, `check`, `schemas validate` derive
from data. Only `pen` writes back to the fleet (via PRs).

## Invariants

Each state has a sharp rule about what it cannot do:

- **scan** does not fetch file contents. It only builds the index.
- **pull** does not analyse or transform. It materialises tracked files into the cache.
- **search / build / check** never write. They are pure projections of the cache.
- **pen** is the only writer. It never short-circuits the cache; pen workspaces are
  full shallow clones, isolated per transaction.

These aren't just style guidelines — they're the reason the system composes. A flaky
network during `scan` leaves the cache untouched; a failed codemod leaves the fleet
untouched; an analysis is always reproducible against a given cache snapshot.

## Why the separation matters

Consider a simple counter-design where `search` could transparently `pull` missing
files on demand. Convenient! But now:

- `search` has a network dependency, so it's slow and flaky.
- Results depend on wall-clock state: two back-to-back searches can disagree.
- Incremental scanning becomes intractable — you can't tell the cache from the fleet.

Explicit state separation trades a little up-front ceremony (`scan` + `pull` before
anything else) for a much cleaner composition story downstream. It also lets
local-only analysis run at native speed; the [Design spec](https://cog.spin.systems/fleet-ops-devtool-design)
calls this the *fast-path local-only mode*.

## Where each state lives

- Scan index & cache: `~/.cache/nave/` (or `$XDG_CACHE_HOME/nave/`)
- Schemas: under the same cache root, in a separate namespace
- Pens: `~/.local/share/nave/pens/<pen-name>/` (or `$XDG_DATA_HOME/nave/pens/`)
- User config: `~/.config/nave.toml`

See [Cache](cache.md) and [Pens](pens.md) for layout details.
