# Command relationships

Nave's CLI is a pipeline, not a collection of independent tools. Commands produce
state that other commands consume; running them out of order errors early.

## Dependency graph

```
init ──► scan ──► pull ──► search
                      │        ▲
                      │        │
                      └──► build
                              │
                              ▼
                    schemas pull ──► schemas validate
                                          ▲
                                          │
                                       pen create ──► pen exec ──► (pen run 🚧)
                                          │               │
                                          ├──► pen status │
                                          ├──► pen list   │
                                          ├──► pen show   │
                                          │               ▼
                                          └──► pen sync   pen clean
                                                          pen revert
                                                          pen reinit
                                                          pen rm
```

## Preconditions

Each command checks its preconditions before doing work:

| Command                   | Requires                           |
|---------------------------|------------------------------------|
| `scan`                    | `init` has run (config exists)     |
| `pull`                    | `scan` has run (index exists)      |
| `search` / `build` / `check` | `pull` has run (cache populated) |
| `schemas validate`        | Schema cache + pen exists          |
| `pen create`              | `pull` has run; filter matches >0  |
| `pen exec` / `sync` / etc.| Pen exists                         |

Preconditions that fail produce a clear error explaining how to satisfy them (e.g.
"cache root /home/u/.cache/nave does not exist; run `nave scan` + `nave pull` first").

## State layering

From bottom to top:

1. **Fleet** — the remote. Nave reads from it, pens write to it.
2. **Scan index** — what exists. Produced by `scan`, consumed by `pull`.
3. **Cache** — what's in the files. Produced by `pull`, consumed by `search`/`build`/`check`.
4. **Templates / hole reports / check outcomes** — pure functions of the cache.
5. **Pens** — transaction workspaces on top of the cache, with their own state.

Each layer is strictly above its dependencies: writes never flow down.

## Related concept pages

- [Core primitives](../../concepts/core-primitives.md) — core primitives and invariants.
- [Operations](../../concepts/operations.md) — read/validate/write layering.
- [Pens](../../concepts/pens.md) — transaction semantics.
