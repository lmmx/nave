# ++"nave build"++

Anti-unify configuration files across repos to expose shared templates and drift.

## Usage

```bash
--8<-- "docs/_snippets/cli/build.txt"
```

## What it does

1. Groups tracked files by their logical kind (by glob, so all
   `.github/dependabot.yml` files form one group, all `pyproject.toml` form another).
2. Anti-unifies each group: walks the parsed trees in parallel; agreement becomes a
   literal in the template, disagreement becomes a hole.
3. Annotates each hole with distinct values, cohort sizes, and source hints
   (constant-when-present, derived-from-repo-name).

See [Templates](../../concepts/templates.md) for the underlying model.

## Output

Text mode (default):

```
━━ .github/dependabot.yml ━━
  instances: 9

  template:
    updates:
      - cooldown?: {"default-days":7}
        directory: "/"
        package-ecosystem: ⟨?0⟩
        schedule:
          interval: ⟨?1⟩
    version: 2

  holes:
    updates[package-ecosystem,schedule].package-ecosystem  [string]  9/9
        8× "github-actions"
        1× "cargo"
    updates[package-ecosystem,schedule].schedule.interval  [string]  9/9
        6× "weekly"
        3× "monthly"

  profiles: (7 concepts, 3 non-trivial)
    Profile 1  (5 repos: polite, polars-fastembed, polars-genson, … +2)
      updates[].package-ecosystem = "github-actions"
      updates[].schedule.interval = "weekly"
    Profile 2  (3 repos: trusty-pub, ossify, clickpydeps)
      updates[].package-ecosystem = "github-actions"
      updates[].schedule.interval = "monthly"
    Profile 3  (1 repos: comrak)
      updates[].package-ecosystem = "cargo"
      updates[].schedule.interval = "weekly"
```

JSON mode (`--json`) produces the same structure as a nested `BuildReport` object.

## Narrowing

Three independent narrowing mechanisms:

| Flag                  | Scope                                          |
|-----------------------|------------------------------------------------|
| `--filter`            | Skip groups whose pattern doesn't match        |
| `--where`             | Pre-filter on files (substring terms)          |
| `--match`             | Pre-filter on tree structure (predicates)      |
| `--co-occur`          | Anti-unify subtrees, not whole files           |
| `--relevant-profiles` | Show only profiles matching `--match` values   |

They compose. Typical usage:

```bash
# Drift only among CI workflows that mention both maturin and pytest,
# and only in the subtrees where both appear.
nave build \
  --filter workflows \
  --where workflow:maturin \
  --where workflow:pytest \
  --co-occur
```

## `--co-occur` semantics

With two or more `--where` terms, `--co-occur` changes the input to anti-unification:

- **Without `--co-occur`** — each whole matching file is an anti-unification input.
- **With `--co-occur`** — the input is the *deepest non-root object ancestor* shared
  by a match of the first (anchor) term and at least one match of every other term.

This is the right tool when a single file contains many independent sections and you
only care about one. Anti-unifying whole workflow files (for example) will usually
drown the signal you want in inter-job variation.

Requires ≥ 2 `--where` terms.

## Profiles

After anti-unification, `nave build` runs formal concept analysis (FCA) on the
hole observations to discover **configuration profiles** — maximal sets of
hole-value bindings shared by subsets of instances.

Each profile is a [formal concept][fca]: a set of repos (the extent) and the
hole-value bindings they all share (the intent). Profiles are displayed as a
lattice — when a profile refines another (its repos are a subset), only the
new bindings (the delta) are shown, with a `(refines Profile N)` annotation.

[fca]: https://en.wikipedia.org/wiki/Formal_concept_analysis

Root profiles (those with no parent in the lattice) show their full intent.
Refinements show only what's new. Structural differences (optional keys
present in one subgroup but not another) are summarised as
`(N fewer optional keys)`.

Profiles with multiple parents represent repos at the intersection of
several groups — their configuration is the combination of all parent
profiles.

### `--relevant-profiles`

When `--match` predicates are provided, `--relevant-profiles` filters the
profile display to only show profiles where at least one binding's value
satisfies a `--match` predicate. This focuses the output on the variation
you searched for.

```bash
nave build \
  --where workflow:uv \
  --match 'workflow:run*=pytest' \
  --relevant-profiles
```

This shows all holes across the full workflow template, but only displays
profiles whose bindings involve a value containing "pytest".

## Hole kinds

See [Templates § Hole kinds](../../concepts/templates.md#hole-kinds).

- **Scalar** — leaf value varies.
- **OptionalKey** — key present in some, absent in others. Report shows `N/M optional`.
- **Shape** — structural disagreement (rare).
