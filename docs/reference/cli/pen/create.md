# ++"nave pen create"++

Create a pen by filtering the fleet and cloning matching repos.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen/create.txt"
```

## What it does

1. Resolves the filter against the cache (same engine as [++"nave search"++](../search.md)).
2. Generates a pen name if not given (`nave/<slug>` from the first term, truncated
   to 20 chars, suffixed with `-<n>` on clash).
3. Creates `~/.local/share/nave/pens/<name>/` as the pen root.
4. For each matching repo:
   a. Shallow-clones the default branch (non-sparse — pens are full clones).
   b. Creates a branch `<pen-name>` at HEAD.
5. Writes a `pen.toml` manifest recording the filter, the repo list, and the
   sync timestamp.

## Naming

The name doubles as the branch name on each repo, so it must be:

- Slug-safe (lower-case, alphanumeric, hyphens).
- Not an existing branch on any of the target repos.
- Unique among local pens.

Auto-generation follows the algorithm:

1. Strip scope prefix from the first term (`workflow:a-b` → `a-b`).
2. Truncate to 20 characters.
3. Prefix with `nave/`.
4. Append `-<n>` if the name already exists.

You can override with `--name`. The `nave/` prefix is always retained.

## Examples

```bash
# Auto-named pen from one term
nave pen create workflow:maturin
# → creates nave/maturin

# With explicit name
nave pen create --name nave/drop-py38 \
  --match 'file:pyproject.toml project.requires-python~3.8' \
  pyproject

# Case-insensitive and narrowing with --match
nave pen create -i MATURIN \
  --match 'file:pyproject.toml tool.maturin~'
```

## What it doesn't do

- Does not run any codemod — use [++"nave pen exec"++](exec.md) (or the planned [++"nave pen run"++](run.md)).
- Does not push branches — those live locally until [++"nave pen exec --push-changes"++](exec.md) or [++"nave pen run"++](run.md).
- Does not open PRs.

Freshly created pens have run state `not-run` and clean working trees.
