# CLI overview

The [++"nave"++](nave.md) CLI maps directly onto the lifecycle stages. Each subcommand belongs to
exactly one group, and each group corresponds to a [state layer](../../concepts/operations.md).

## Command groups

### Setup

- [++"nave init"++](init.md) — create `~/.config/nave.toml`.

### Read (data)

- [++"nave scan"++](scan.md) — enumerate repos; index tracked files.
- [++"nave pull"++](pull.md) — sparse-checkout tracked files into the cache.

### Read (analysis)

- [++"nave search"++](search.md) — substring and structural queries.
- [++"nave build"++](build.md) — anti-unified templates and hole reports.

### Validate

- [++"nave check"++](check.md) — parse and round-trip tracked files.
- [++"nave schemas"++](schemas.md) — manage schema cache; validate pens.

### Write

- [++"nave pen"++](pen.md) — pens, subcommands for the full lifecycle.

## Global conventions

- `--json` — most commands support structured output for scripting.
- `NAVE_LOG=debug` — verbose logging (`tracing-subscriber` EnvFilter syntax).
- `--help` on any subcommand prints its usage.

## Shared grammar

Two pieces of grammar are used across multiple commands:

- **Terms** (`search`, `pen create`, `build --where`): `[scope:]value[|value...]`.
  See [Query language](../../concepts/queries.md).
- **Match predicates** (`search --match`, `build --match`, `pen create --match`):
  `[scope:]path op literal` where `op` is `=` or `~`.
  See [Query language § Structural predicates](../../concepts/queries.md#structural-predicates).
