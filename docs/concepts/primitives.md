# Primitives

Nave has a few core primitives — most importantly the **fleet** and the **pen**.

It also has certain operations that are important to understand: the **scan** of the remote,
the fleet **cache**, and building the **templates** with **holes** which are used for search queries.

| Primitive | What it is                                      | Scope         |
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

The phases have well-defined boundaries:

- **scan** only builds the index of the remote git repos, and does not fetch their file contents.
- **pull** materialises tracked files into the cache, and does not analyse or transform.
- **search / build / check** are pure projections of the cache, and are read-only.
- **pen** is the only writer. It never short-circuits the cache; pen workspaces are
  full shallow clones, isolated per transaction.

These boundaries are the reason the system composes well.

A flaky network during `scan` leaves the cache untouched; a failed codemod leaves the fleet untouched;
an analysis is always reproducible against a given cache snapshot.

Explicit separation of these phases trades a little up-front ceremony (`scan` + `pull` before anything else)
for a much cleaner user experience downstream.

It also lets local-only analysis run at native speed; in the [design spec](https://cog.spin.systems/fleet-ops-devtool-design)
we called this the *fast-path local-only mode* but in practice it's just a nice aspect that's now default.

## Where nave's state lives

- Scan index & fleet cache: `~/.cache/nave/` (or `$XDG_CACHE_HOME/nave/`)
- Schemas: under the same cache root, in a separate namespace
- Pens: `~/.local/share/nave/pens/<pen-name>/` (or `$XDG_DATA_HOME/nave/pens/`)
- User config: `~/.config/nave.toml`

See [Cache](cache.md) and [Pens](pens.md) for layout details.
