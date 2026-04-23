# Queries

Multiple commands in nave allow you to query for search terms,
using a simple query language based around terms and where they can be found.

## Query language

A small, uniform filter grammar used by `search`, `build`, and `pen create`.

- Space-separated terms AND together;
- `|` inside a term ORs alternatives;
- `scope:` prefixes restrict matching to a file kind.

## Grammar

```
query      := term (WS term)*
term       := [scope ':'] value ('|' value)*
scope      := 'workflow' | 'file' | ...
value      := any non-whitespace literal
```

## Semantics

| Form               | Meaning                                                        |
|--------------------|----------------------------------------------------------------|
| `maturin`          | Substring match anywhere in any tracked file                   |
| `workflow:pytest`  | Substring match, restricted to `.github/workflows/*.yml`       |
| `a|b`              | Either `a` or `b` (OR)                                         |
| `a b`              | Both `a` and `b` must match (AND across terms)                 |
| `workflow:a|b`     | `a` or `b`, but only within workflow files                     |

Case-insensitive matching is a flag: `-i` / `--ignore-case` on `search` and `pen create`.

## Scopes

- `file:` — match only in a file whose basename contains the value.
- `workflow:` — match only in `.github/workflows/*.yml`.
- No prefix — match anywhere in any tracked file.

The scope set is small and fixed. It exists to remove the most common false positives
(e.g. "the string `pytest` appears in `pyproject.toml` as a dev dependency, but I meant
the CI workflow").

## Structural predicates

Substring search is a blunt tool — it finds needles in text without caring about
structure. For "does this repo's `pyproject.toml` have `requires-python >= 3.10`?" you
want a *structural* query.

`--match` takes a predicate of the form:

```
[scope:]path op literal
```

where `op` is `=` (exact) or `~` (substring). The `path` is a dotted address into the
parsed tree (see [Addresses](#addresses) below).

Examples:

```bash
# repos whose pyproject targets Python >= 3.10
nave search --match 'file:pyproject.toml project.requires-python~3.10'

# repos whose dependabot schedules are weekly
nave search --match 'file:.github/dependabot.yml updates[0].schedule.interval=weekly'
```

`--match` composes with plain terms and with `--co-occur` (on `build`).

## Co-occurrence

Plain terms AND at the *file* level: a repo matches if each term finds its target
somewhere in some tracked file. That's too loose when you want "this term and this
other term in the same block".

`--co-occur` (available on `build`) anti-unifies subtrees rather than whole files.
A co-occurrence site is the deepest non-root object ancestor shared by a match of
the first term and at least one match of every other term. Requires ≥ 2 `--where` terms.

See [`nave build`](../reference/cli/build.md) for worked examples.

## Addresses

Paths into parsed trees use the following grammar:

- Dot notation for object keys: `project.name`, `tool.coverage.run.source`
- Bracket notation for arrays: `updates[0]`, `source[0]`
- Mixed freely: `tool.isort.known_first_party[0]`

Addresses show up in three places: `--match` predicates, `build` hole reports, and
`search --output holes`. The grammar is identical in all three.
