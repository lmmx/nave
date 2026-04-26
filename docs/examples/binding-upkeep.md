## Surveillance over Rust-binding repos

A niche but illustrative example: if you maintain Python packages that wrap Rust
crates via PyO3 / maturin, bumping the underlying crate version isn't sufficient.
A new crate version might add fields to a public struct that your bindings no
longer cover, and "does it build?" won't catch it.

This motivates an informational fleet operation: surveillance, not mutation.

### Goal

For each Rust-binding repo: detect when the upstream crate has gained public API
surface that the bindings don't expose.

Tools involved:

- [`cargo public-api diff`](https://github.com/cargo-public-api/cargo-public-api) —
  diff the Rust public API between versions.
- [`griffe check`](https://github.com/mkdocstrings/griffe) — the Python-side
  equivalent for public API diffing.

### Step 1: identify binding repos

Binding repos share two markers: a `[tool.maturin]` block in `pyproject.toml`, and a
dependency on a Rust crate from within the repo's Rust source.

```bash
nave search \
  --match 'pyproject:tool.maturin' \
  workflow:maturin
```

- This outputs a list of repo names (default `--output` is repos)

The predicate `pyproject:tool.maturin` is a presence check — it matches whenever
the `tool.maturin` path exists in the parsed pyproject, regardless of its contents.
This is a true structural presence check, not a substring match that could
false-positive on comments or unrelated text.

### Step 2: tabulate the versions

```bash
nave search \
  --match 'pyproject:tool.maturin' \
  --output holes \
  --explain
```

This emits every occurrence of the `tool.maturin` block across matching repo's `pyproject.toml`,
grouped by structural address.

Each entry shows:

- the file and address
- how many repos matched
- per-repo values at that location

Example:

```
pyproject.toml  tool.maturin  (13 hits)
    lmmx/ansi-to-html :: pyproject.toml
        {"features":["pyo3/extension-module"]}
    lmmx/czkawka :: pyproject.toml
        {"features":["pyo3/extension-module"],"module-name":"czkawka._czkawka",...}
```

This is useful for quickly seeing how binding repos are configured (which only set `features`,
which define `module-name` or `python-source`, which introduce additional flags or diverge
from the common pattern).

At this stage you're not extracting crate versions yet but you're getting an overview of
the configuration surface of the bindings so you know what shapes your later
analysis needs to handle.

### Step 3: build a pen for surveillance

These overviews don't make any interventions in the code, but we can still use a pen to collect them:
if you need isolated workspaces to run `cargo public-api` in each repo, and you want the results
aggregated (even if we don't plan on pushing changes based on what we find).

```bash
nave pen create --name nave/surveil-bindings \
  --match 'pyproject:tool.maturin'
```

Then:

```bash
nave pen exec nave/surveil-bindings -- \
  cargo public-api diff --from-git=main --to-git=HEAD
```

(Or whichever diff strategy suits.)

Collect the per-repo outputs; flag any that show new public items since the last
binding release.

## Why this fits Nave

The general approach here is to carry out surveillance over a fleet subset,
with isolated execution per repo, aggregated results.

The pen concept covers this kind of task even when the use case isn't applying codemods.
