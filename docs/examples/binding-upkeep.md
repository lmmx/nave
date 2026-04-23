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
  --match 'file:pyproject.toml tool.maturin~' \
  workflow:maturin
```

The `--match` predicate is a presence check — the `~` substring match against an
empty string matches whenever the address exists.

### Step 2: tabulate the versions

```bash
nave search \
  --match 'file:pyproject.toml tool.maturin~' \
  --output holes \
  --explain \
  | rg 'Cargo.toml'
```

This surfaces every `Cargo.toml` field pointed to by the binding repos — useful for
seeing which crate versions are pinned across the fleet.

### Step 3: build a pen for surveillance

Surveillance isn't a codemod, but the pen model still fits: you need isolated
workspaces to run `cargo public-api` in each repo, and you want the results
aggregated.

```bash
nave pen create --name nave/surveil-bindings \
  --match 'file:pyproject.toml tool.maturin~'
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

The specifics here are less important than the general shape: surveillance over a
fleet subset, with isolated execution per repo, aggregated results. The pen model
covers it natively even though the use case isn't "apply a codemod".
