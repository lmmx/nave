# Queries

Multiple commands in [++"nave"++](../reference/cli/nave.md) allow you to query for search terms,
using a simple query language based around terms and where they can be found.

## Query language

A small, uniform filter grammar used by `search`, `build`, and `pen create`.

- Space-separated terms AND together;
- `|` inside a term ORs alternatives;
- `scope:` prefixes restrict matching to files whose tracked-path pattern contains the scope as a substring.

## Grammar

```
query      := term (WS term)*
term       := [scope ':'] value ('|' value)*
scope      := substring of a tracked-path pattern
value      := any non-whitespace literal
```

## Semantics

| Form               | Meaning                                                           |
|--------------------|-------------------------------------------------------------------|
| `maturin`          | Substring match anywhere in any tracked file                      |
| `workflow:pytest`  | Substring match, restricted to patterns containing `workflow`     |
| `a|b`              | Either `a` or `b` (OR)                                            |
| `a b`              | Both `a` and `b` must match (AND across terms)                    |
| `workflow:a|b`     | `a` or `b`, within patterns containing `workflow`                 |

Case-insensitive matching is a flag: `-i` / `--ignore-case` on `search` and `pen create`.

## Scopes

A scope prefix (`scope:value`) restricts a term to files whose
tracked-path glob pattern contains `scope` as a substring. There is no
fixed set of scopes — any substring of a pattern in your
`tracked_paths` config works.

With the default `tracked_paths`, useful scopes include:

- `pyproject:` — matches `pyproject.toml`
- `Cargo:` — matches `Cargo.toml` (note: case-sensitive; `cargo:` doesn't match without `-i`)
- `workflow:` — matches `.github/workflows/*.yml`/`*.yaml`
- `dependabot:` — matches `.github/dependabot.yml`/`.yaml`
- `.toml:` — matches every TOML pattern
- `.yml:` — matches every YAML pattern

If you add new patterns to `tracked_paths`, new scopes become available
automatically.

**Caveat.** Because scopes are substrings of pattern globs, they're
sensitive to both your `tracked_paths` config and to case. `cargo:`
doesn't match the default `Cargo.toml` pattern — you need `Cargo:` or
`-i`. If you rename a pattern in your config, any scoped queries that
relied on the old name will silently stop matching until updated.

## Structural predicates

Substring search is a blunt tool — it finds needles in text without caring about
structure. For "does this repo's `pyproject.toml` have `requires-python >= 3.10`?" you
want a *structural* query.

`--match` takes a predicate of the form:

```
[scope:] [!] path [op literal]
```

Operators:

| Form          | Meaning                                  |
|---------------|------------------------------------------|
| `path = v`    | exact string equality                    |
| `path != v`   | not equal                                |
| `path ^= v`   | starts with                              |
| `path $= v`   | ends with                                |
| `path *= v`   | contains (substring)                     |
| `path`        | path is present (resolves to ≥1 value)   |
| `!path`       | path is absent (resolves to zero values) |

The `path` is a dotted address into the parsed tree with optional
array-element wildcards — see [Addresses](#addresses) below.

Examples:

```bash
# repos whose pyproject mentions 3.10 in requires-python
nave search --match 'pyproject:project.requires-python*=3.10'

# repos where any dependabot update is weekly
nave search --match 'dependabot:updates[].schedule.interval=weekly'

# repos still using the deprecated maturin-action
nave search --match 'workflow:uses^=PyO3/maturin-action@'

# repos that declare a [tool.maturin] block
nave search --match 'pyproject:tool.maturin'

# workflow steps using maturin-action with no packages-dir set
nave search \
  --match 'workflow:uses^=PyO3/maturin-action@' \
  --match 'workflow:!with.packages-dir'

# dependabot configs where at least one update has a cooldown block
nave search --match 'dependabot:updates[].cooldown'
```

Absence predicates emit the *anchor* address — the object where the
missing field would live — because there's no scalar address to point
at for something that doesn't exist. Presence and binary-op
predicates emit the concrete address of the resolved value.

`--match` composes with plain terms and with `--co-occur` (on `build`).
At least one of a positional term or `--match` is required.

### Filtering profiles by predicate

`--relevant-profiles` (on `build`) uses the same `--match` predicates to
filter which profiles are displayed. A profile is shown only if at least one
of its bindings' values satisfies a `--match` predicate.

```bash
# Show the full workflow template but only profiles involving pytest
nave build \
  --where workflow:uv \
  --match 'workflow:run*=pytest' \
  --relevant-profiles
```

This does not change which repos are included or how anti-unification works —
it only filters the profile display.

## Co-occurrence

Plain terms AND at the *file* level: a repo matches if each term finds its target
somewhere in some tracked file. That's too loose when you want "this term and this
other term in the same block".

`--co-occur` (available on `build`) anti-unifies subtrees rather than whole files.
A co-occurrence site is the deepest non-root object ancestor shared by a match of
the first term and at least one match of every other term. Requires ≥ 2 `--where` terms.

See [++"nave build"++](../reference/cli/build.md) for worked examples.

## Addresses

Paths into parsed trees use the following grammar:

- Dot notation for object keys: `project.name`, `tool.coverage.run.source`
- Bracket notation for arrays: `updates[0]`, `source[0]`
- Array wildcard `[]` for one-to-many resolution: `updates[].schedule.interval`
  picks out the interval of every element of `updates`
- Mixed freely: `tool.isort.known_first_party[0]`, `jobs.release.steps[].uses`

Addresses show up in three places: `--match` predicates, `build` hole reports, and
`search --output holes`. The grammar is identical in all three.

Malformed brackets (`foo[`, `foo[abc]`) are rejected at parse time rather than
silently matching nothing.
