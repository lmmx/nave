# Rewrites

A rewrite is a declarative transformation applied to tracked files in a
pen. The transformation is recorded as an op in `pen.toml` and carried
out by [++"nave pen rewrite"++](../reference/cli/pen/rewrite.md).

Rewrites are the *declarative* half of pen mutation. The *imperative*
half is [++"nave pen exec"++](../reference/cli/pen/exec.md), which
shells out to arbitrary commands. The declarative model exists because
most fleet codemods are structurally simple — change this value here,
delete this key there — and gain real safety from being expressed
against the parsed tree rather than as text edits.

## The op model

Each op has an `id`, a `selector`, and an `action`:

```toml
[[ops]]
id = "to-monthly"
selector = { kind = "predicate", predicate = "dependabot:updates[].schedule.interval=weekly" }
action = { kind = "set", value = "monthly" }
status = "pending"
```

### Selectors

The only selector kind is `predicate`, which uses the same predicate
grammar as [`--match`](queries.md#structural-predicates). The predicate
is evaluated per file; every concrete address it resolves to becomes a
target for the action.

### Actions

Four action kinds:

| Kind             | Effect                                                       |
|------------------|--------------------------------------------------------------|
| `set`            | Replace the value at each matched address                    |
| `delete`         | Remove the key (in mappings) or element (in arrays)          |
| `rename_key`     | Rename the leaf key; only valid for keys, not array elements |
| `insert_sibling` | Add a new key=value into the parent of each matched address  |

`Action`s are JSON-typed, so values are written as JSON in TOML:

```toml
action = { kind = "set", value = 7 }
action = { kind = "set", value = "weekly" }
action = { kind = "set", value = { "default-days" = 7 } }
action = { kind = "insert_sibling", key = "cooldown", value = { "default-days" = 7 } }
```

## Atomicity

Rewrites are **atomic per repo by default**. For each repo, every op is
staged in memory, post-mutation schema validation is run, and then all
files are written. If anything fails — a malformed predicate, a missing
address, a schema violation — the working tree is left untouched.

Atomicity does not extend across repos: a rewrite that succeeds in 8
repos and fails in 2 leaves 8 working trees modified and 2 untouched.
The pen-level status reflects this as `partial`.

### Opting out: `--no-rollback`

`--no-rollback` writes incrementally as ops succeed. If an op fails
partway, prior writes for that repo remain in the working tree. The
rewriter prints a warning before any work begins:

```
warning: --no-rollback set; failed rewrites will leave partial changes in working tree
```

A failed `--no-rollback` run records the failed op in the repo's state
and leaves the working tree dirty. The next `pen rewrite` invocation
hits the dirty-tree gate and refuses to proceed until the user resolves
the partial state via `pen clean` or by committing.

## State

State is split between three places:

**`pen.toml`** — pen-level aggregate status per op:

```toml
[[ops]]
id = "to-monthly"
status = "applied"   # pending | applied | partial | failed
```

**`state/<owner>__<repo>/ops.toml`** — per-repo live state. Presence in
`[ops]` means the op is applied for this repo.

```toml
[ops.to-monthly]
applied_at = "2026-04-27T14:32:11Z"
```

The `[failed]` table only appears under `--no-rollback`.

**`state/<owner>__<repo>/run-log.toml`** — append-only history of every
attempt, with file-level detail and any failure reasons.

**`state/<owner>__<repo>/logs/<run-id>/`** — per-run log artefacts:
`<op-id>.{stdout,stderr,err}` for each op that failed. The `err` file
holds the structured error (parse failures, validation errors); the
stdout/stderr files exist for layout consistency with future ops that
shell out.

The split design means workers writing to `state/<repo>/` are
guaranteed disjoint and can run in parallel without locking. Pen-level
aggregation happens once at run end on the orchestrator.

## Pen-level status semantics

Computed from per-repo state at the end of every run:

| Status    | Meaning                                                       |
|-----------|---------------------------------------------------------------|
| `pending` | No repo has applied this op, no `--no-rollback` failures      |
| `applied` | Every in-scope repo has applied this op                       |
| `partial` | Some repos have applied, others haven't                       |
| `failed`  | At least one repo has a `--no-rollback` failure for this op   |

`partial` is the common "more work to do" state under default rollback.
`failed` is reachable only via `--no-rollback`.

## Composition with other commands

`pen rewrite` only updates the working tree. It does not commit, push,
or open PRs. Those concerns live in:

- [++"nave pen exec --commit --push-changes"++](../reference/cli/pen/exec.md) — manual commit/push of an arbitrary command's output
- [++"nave pen run"++](../reference/cli/pen/run.md) (planned) — declarative codemod orchestration: rewrite + commit + push in one step

Until `pen run` ships, the typical flow is:

```bash
nave pen rewrite my-pen          # update working trees
nave pen status my-pen           # confirm changes look right
nave pen exec my-pen --commit -m "apply rewrites"
nave pen exec my-pen --push-changes -- true  # push without further changes
```

## What rewrites don't do

- They don't preserve comments or formatting in v1. The TOML and YAML
  serialisers re-render from the parsed AST, which loses these. A
  swap to `toml_edit` is planned and will fix the TOML side.
- They don't compute new values from old ones. `Set` takes a literal;
  there's no transform-the-existing-value action yet.
- They don't run imperative code. Use [++"nave pen exec"++](../reference/cli/pen/exec.md) for that.
