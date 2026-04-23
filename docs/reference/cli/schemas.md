# `nave schemas`

Manage the JSON Schema cache and validate tracked files.

## Usage

```
Manage the JSON Schema cache and validate tracked files

Usage: nave schemas <COMMAND>

Commands:
  pull      Populate the schema cache based on tracked paths
  list      List schemas and their cache status
  validate  Validate tracked files in a pen against their schemas
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## Subcommands

### `nave schemas pull`

```
Populate the schema cache based on tracked paths

Usage: nave schemas pull [OPTIONS]

Options:
      --refresh  Re-fetch all schemas even if cached
  -h, --help     Print help
```

Fetches JSON Schemas for each schema kind that applies to the current
`tracked_paths`. Network-dependent; failures are logged but non-fatal (so that
`nave init` completes offline).

### `nave schemas list`

```
List schemas and their cache status

Usage: nave schemas list [OPTIONS]

Options:
      --json  Emit JSON instead of text
  -h, --help  Print help
```

One line per schema:

```
✓ dependabot            /home/u/.cache/nave/schemas/dependabot.json (12345 B)
✓ github-workflow       /home/u/.cache/nave/schemas/github-workflow.json (67890 B)
✓ github-action         /home/u/.cache/nave/schemas/github-action.json (23456 B)
✓ pyproject             /home/u/.cache/nave/schemas/pyproject.json (34567 B)
```

`·` for missing, `✓` for cached.

### `nave schemas validate`

```
Validate tracked files in a pen against their schemas

Usage: nave schemas validate [OPTIONS] <PEN>

Arguments:
  <PEN>  Pen name to validate

Options:
      --check-actions  Also validate workflow action `with:` blocks
                       against upstream `action.yml`. Requires network
                       for first-time ref resolution.
      --fail-fast      Stop at the first failing file
      --json           Emit JSON instead of text
  -h, --help           Print help
```

Validates every tracked file in the named pen against its schema. Output is per-repo
with a progress bar (interactive mode) or line-oriented (`--json`).

Failure types:

- **Schema errors** — the file doesn't match the JSON Schema.
- **Action errors** (with `--check-actions`) — a workflow step's `with:` block
  references inputs the target action doesn't declare (missing required, unknown,
  or deprecated).

Exit code is non-zero if any failures exist. See [Schemas](../../concepts/schemas.md)
for the concept.

## Sources

Schemas are pulled from their canonical locations by default:

| Schema            | Source                                                                       |
|-------------------|------------------------------------------------------------------------------|
| `dependabot`      | `https://www.schemastore.org/dependabot-2.0.json`                            |
| `pyproject`       | `https://www.schemastore.org/pyproject.json`                                 |
| `github-workflow` | `https://www.schemastore.org/github-workflow.json`                           |
| `github-action`   | `https://www.schemastore.org/github-action.json`                             |

You can override these in `~/.config/nave.toml`:

```toml
[schemas.sources]
dependabot = "https://example.com/my-dependabot-schema.json"
```

Cargo.toml has no SchemaStore entry yet (tracked in
[rust-lang/cargo#12883](https://github.com/rust-lang/cargo/issues/12883)); it's
parsed by `check` but not schema-validated.
