# `nave pen rm`

Remove a pen's local workspace and definition.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen/rm.txt"
```

## What it does

1. Deletes `~/.local/share/nave/pens/<name>/` and its contents.
2. Removes the pen from any local manifest.

Local-only by default. Remote branches (if any were pushed) are **not** deleted.

## Safety

If any repo in the pen has uncommitted work, `rm` aborts. Pass `--allow-dirty` to
remove anyway. This is a hard deletion — there's no recycle bin.

## Remote branches

Pushed pen branches stay on the remote until deleted manually, or (planned) by a
`--purge` flag:

```bash
# Planned 🚧
nave pen rm --purge nave/lowest-direct
```

`--purge` would delete the corresponding branch on each remote, with per-repo
confirmation (overridable by `--no-interactive`).

Until that lands, the pattern is:

```bash
# Drop PRs first (if any)
gh pr list --head nave/lowest-direct --json number --jq '.[].number' | \
  xargs -I{} gh pr close {}

# Delete remote branches
for repo in $(nave pen show nave/lowest-direct --json | jq -r '.repos[] | "\(.owner)/\(.name)"'); do
  gh api -X DELETE "repos/$repo/git/refs/heads/nave/lowest-direct"
done

# Then local cleanup
nave pen rm nave/lowest-direct
```
