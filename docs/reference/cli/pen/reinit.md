# ++"nave pen reinit"++

Rebuild the pen branch from origin's default branch.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen/reinit.txt"
```

## What it does

For each repo:

1. Fetch origin's default branch.
2. Delete the local pen branch.
3. Recreate it at the fetched head.
4. Reset working tree to the new branch.

Effect: every repo in the pen is back to a pristine state matching what's currently
on the remote's default branch. This is stronger than `revert`: it also pulls in any
changes the default branch has received since the pen was created.

## When to use it

- The default branch has advanced and you want the pen to start from the new tip.
- The pen has gone sufficiently off the rails that `revert` isn't enough.
- You want to retry a codemod from scratch against the current remote state.

## Dirty trees

Same as `revert`: defaults to refusing on a dirty tree; `--allow-dirty` overrides.

## Not a sync

`reinit` only touches local branches. It does not re-evaluate the filter or update
the pen's repo list. If the fleet has gained or lost matching repos, run `sync`
afterwards (or before).
