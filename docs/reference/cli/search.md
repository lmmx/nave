# `nave search`

Search cached repositories for patterns across tracked files.

## Usage

```
Search cached repos for substring patterns across tracked files

Usage: nave search [OPTIONS] <TERMS>...

Arguments:
  <TERMS>...  One or more search terms. Each is `[scope:]value[|value...]`.

Options:
      --match <PREDICATE>  Structural predicate of the form
                           `[scope:]path op literal`, where `op` is
                           `=` (exact) or `~` (substring). Same syntax as
                           `nave build --match`.
      --output <OUTPUT>    Output projection [default: repos]
                           [possible values: repos, files, holes]
      --json               Emit JSON instead of the projected text form
      --count              Print only the count of matches
      --explain            Show which files satisfied each term per repo
  -i, --ignore-case        Case-insensitive substring match (ASCII)
      --sort <SORT>        Sort results by a key
                           [possible values: pushed-at, name]
      --limit <LIMIT>      Limit to the first N results (applied after sorting)
  -h, --help               Print help
```

## Term grammar

Each positional `TERMS` argument is a term:

- `value` — substring match anywhere in any tracked file.
- `scope:value` — scoped substring match (`file:`, `workflow:`).
- `a|b` — OR inside a term.

Terms space-separated are ANDed together. See [Query language](../../concepts/query-language.md).

## Projections

### `--output repos` (default)

One line per repo. Terse. Use when you want a list of repo names.

### `--output files`

One line per matching file, formatted as `owner/repo:path`. A file satisfying
multiple terms prints once.

### `--output holes`

Group by structural address. Requires parsed enrichment under the hood — a little
slower, but tells you where in the file the match landed.

```bash
nave search maturin --output holes | rg -v workflows
```

```
pyproject.toml  build-system.build-backend  (2 hits)
pyproject.toml  tool.maturin                (2 hits)
```

## `--match` predicates

Orthogonal to the positional terms: while terms are substring matches over raw bytes,
`--match` operates on parsed tree structure. They compose (AND).

```bash
# repos with requires-python >= 3.10
nave search --match 'file:pyproject.toml project.requires-python~>=3.10'

# repos where dependabot is weekly
nave search --match 'file:.github/dependabot.yml updates[0].schedule.interval=weekly'
```

See [Query language § Structural predicates](../../concepts/query-language.md#structural-predicates-match).

## Modifiers

- `--count` — skip all output, print only the count of the chosen projection.
- `--explain` — extra per-result detail: which terms matched which files.
- `--sort pushed-at` — most recently touched first.
- `--limit N` — truncate (after sorting).
- `-i` — case-insensitive.

## JSON output

`--json` emits the `SearchReport` structure: an array of repos, each with its hits
(per-term match metadata). Compatible with `jq` pipelines.
