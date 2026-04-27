# ++"nave pen rewrite"++

Apply declarative rewrites defined in a pen's `pen.toml`.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen/rewrite.txt"
```

## What it does

For each repo in the pen (or just `--only` if given):

1. Parse every op's selector predicate. If any are malformed, abort.
2. Verify the working tree is clean (unless `--allow-dirty`).
3. Stage all ops in memory: parse files, plan addresses, apply mutations.
4. Run post-mutation schema validation (unless `--no-validate`).
5. If everything succeeds: write files, record state, append run log.
6. If anything fails: roll back (default) or write partial state and
   record the failure (`--no-rollback`).

`pen rewrite` does **not** commit or push. That's the job of
[++"nave pen exec"++](exec.md) or the planned [++"nave pen run"++](run.md).

## Atomicity

Atomic per-repo by default; `--no-rollback` opts out. See
[Rewrites § Atomicity](../../../concepts/rewrites.md#atomicity) for the
full model.

## Examples

```bash
# Apply every pending op in the pen
nave pen rewrite nave/dependabot-monthly

# Restrict to one op, single repo
nave pen rewrite nave/dependabot-monthly \
  --op to-monthly --only lmmx/comrak

# See what would change without writing
nave pen rewrite nave/dependabot-monthly --dry-run

# See unified diffs of every change
nave pen rewrite nave/dependabot-monthly --diff

# Re-run ops that already applied
nave pen rewrite nave/dependabot-monthly --force

# Skip schema validation (e.g. a schema is broken upstream)
nave pen rewrite nave/dependabot-monthly --no-validate

# Continue past failures, leaving partial state
nave pen rewrite nave/dependabot-monthly --no-rollback
```

## Output

Text mode:

```
pen: nave/dependabot-monthly  run: 20260427T143211Z
✓ lmmx/comrak
    to-monthly — applied
        .github/dependabot.yml
✗ lmmx/polite
    to-monthly — failed: ...
  ↪ rolled back due to op "to-monthly"; see logs at ~/.local/share/nave/pens/dependabot-monthly/state/lmmx__polite/logs/20260427T143211Z/

op statuses:
  to-monthly: Partial
```

Symbols: `✓` committed, `✗` rolled back, `·` no changes.

## Failure recovery

If a `--no-rollback` run fails and leaves a repo dirty, the next
`pen rewrite` will refuse until you resolve the partial state:

```bash
# Discard partial work
nave pen clean my-pen

# Or commit it
cd ~/.local/share/nave/pens/my-pen/repos/owner__repo
git add -A && git commit -m "partial rewrite"
```

Then `pen rewrite` proceeds normally.

## Exit code

Non-zero if any repo rolled back or any op failed. Useful in CI.
