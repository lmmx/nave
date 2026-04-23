# `nave build`

Anti-unify configuration files across repos to expose shared templates and drift.

## Usage

```
Simplify configs across repos into shared templates

Usage: nave build [OPTIONS]

Options:
      --json                     Emit as JSON instead of text
      --filter <FILTER>          Restrict output to groups whose pattern
                                 contains this substring
      --where <TERM>             Narrow the input to files satisfying every
                                 term. Grammar: `[scope:]value[|value...]`,
                                 same as `nave search`.
      --match <PREDICATE>        Structural predicate of the form
                                 `[scope:]path op literal`, where `op` is
                                 `=` (exact) or `~` (substring). Composes
                                 with `--where` and `--co-occur`.
      --co-occur                 Anti-unify the subtrees where `--where`
                                 terms co-occur rather than whole files
  -h, --help                     Print help
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
      - cooldown?: ⟨?0⟩
        directory: "/"
        package-ecosystem: ⟨?1⟩
        schedule:
          interval: ⟨?2⟩
    version: 2

  holes:
    updates[0].cooldown  [optionalkey]  3/9 optional  [constant when present]
        3× {"default-days":7}
    updates[0].package-ecosystem  [string]  9/9
        8× "github-actions"
        1× "cargo"
    updates[0].schedule.interval  [string]  9/9
        6× "weekly"
        3× "monthly"
```

JSON mode (`--json`) produces the same structure as a nested `BuildReport` object.

## Narrowing

Three independent narrowing mechanisms:

| Flag          | Scope                                          |
|---------------|------------------------------------------------|
| `--filter`    | Post-hoc filter on group pattern (output only) |
| `--where`     | Pre-filter on files (substring terms)          |
| `--match`     | Pre-filter on tree structure (predicates)      |
| `--co-occur`  | Anti-unify subtrees, not whole files           |

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

## Hole kinds

See [Templates § Hole kinds](../../concepts/templates.md#hole-kinds).

- **Scalar** — leaf value varies.
- **OptionalKey** — key present in some, absent in others. Report shows `N/M optional`.
- **Shape** — structural disagreement (rare).
