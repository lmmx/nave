## Searching the fleet

Three common shapes of search query, and the projection each one uses.

### Find repos matching a pattern

Default output (`--output repos`):

```bash
nave search maturin workflow:pytest
```

```
lmmx/comrak
lmmx/polars-fastembed
lmmx/page-dewarp
lmmx/polars-luxical
```

One line per repo, only repos where every term matched something.

### Find files within those repos

```bash
nave search maturin workflow:pytest --output files
```

```
lmmx/comrak:.github/workflows/ci.yml
lmmx/comrak:pyproject.toml
lmmx/polars-fastembed:.github/workflows/ci.yml
lmmx/polars-fastembed:pyproject.toml
...
```

A file that satisfies multiple terms prints once; use `--explain` to see *which*
terms matched it.

### Find positions within those files

```bash
nave search maturin workflow:pytest --output holes | rg -v workflows
```

```
pyproject.toml  build-system.build-backend  (2 hits)
pyproject.toml  build-system.requires[0]    (2 hits)
pyproject.toml  dependency-groups.build[0]  (2 hits)
pyproject.toml  dependency-groups.dev[0]    (2 hits)
pyproject.toml  tool.maturin                (2 hits)
```

`--output holes` groups by structural address (see
[Query language § Addresses](../concepts/queries.md#addresses)).
Useful for answering "where in the file?" rather than just "which file?".

Add `--explain` to see the matched repos and snippet per hit.

### Useful combinations

```bash
## Count matches without listing them
nave search maturin workflow:pytest --count

## Sort by most recently pushed
nave search maturin workflow:pytest --sort pushed-at --limit 10

## Case-insensitive
nave search -i MATURIN

## Structural: only repos with requires-python containing 3.10
nave search --match 'file:pyproject.toml project.requires-python~3.10'

## JSON for scripting
nave search maturin --json | jq '.repos[].repo'
```
