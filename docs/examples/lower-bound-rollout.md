## Rolling out `--resolution lowest-direct`

A realistic bulk-edit scenario from the
[design series](https://cog.spin.systems/fleet-ops-modelling-repos): you've read that
CI tests should use `--resolution lowest-direct` alongside `--frozen`, and you want to
roll this out across every repo that has a pytest CI workflow.

### The pattern

The target is the [MCP Python SDK's workflow](https://github.com/modelcontextprotocol/python-sdk/blob/main/.github/workflows/shared.yml#L53):

```yaml
test:
  strategy:
    matrix:
      python-version: ["3.10", "3.11", "3.12", "3.13", "3.14"]
      dep-resolution:
        - name: lowest-direct
          install-flags: "--upgrade --resolution lowest-direct"
        - name: locked
          install-flags: "--frozen"
```

The rationale: `lowest-direct` uses the lowest compatible versions for direct
dependencies (and latest for transitives), catching "my pinned lower bound doesn't
actually work" bugs that pure lockfile runs miss. Full details in
[uv's resolution docs](https://docs.astral.sh/uv/concepts/resolution/#resolution-strategy).

### Step 1: find the target repos

Which repos have pytest in a CI workflow and use `uv`?

```bash
nave search workflow:pytest workflow:uv
```

Narrow further to those whose `pyproject.toml` has a `requires-python` bound worth
testing at the lower end:

```bash
nave search \
  workflow:pytest workflow:uv \
  --match 'pyproject:project.requires-python^=>='
```

### Step 2: see what currently exists

Does any of your fleet already have a `dep-resolution` matrix? If so, you want to
align with it rather than create a second pattern:

```bash
nave search workflow:dep-resolution --explain
```

If zero results, you're greenfielding. If some results, go read those first.

### Step 3: build a template of the current workflows

To see the shared shape of your pytest workflows before you mutate them:

```bash
nave build --filter workflow --match 'workflow:jobs.test*=pytest'
```

This gives you the anti-unified template, with holes showing exactly which parts vary
today. The codemod needs to slot into those holes cleanly.

### Step 4: create a pen

```bash
nave pen create \
  --name nave/lowest-direct \
  workflow:pytest workflow:uv
```

This clones the matching repos into `~/.local/share/nave/pens/lowest-direct/` and
creates a branch `nave/lowest-direct` in each. The cache is untouched.

Inspect:

```bash
nave pen show nave/lowest-direct
nave pen status nave/lowest-direct
```

### Step 5: apply the edit

🚧 Declarative codemods are still under design. For now, use `pen exec` to run an
arbitrary command in each repo:

```bash
nave pen exec nave/lowest-direct --commit --message \
  "ci: add --resolution lowest-direct to test matrix" -- \
  python path/to/edit-workflow.py
```

`--commit` commits the result in each repo; add `--push-changes` to push branches.
Validate before pushing:

```bash
nave schemas validate nave/lowest-direct --check-actions
```

`--check-actions` is important here — you're editing workflow files, and a mistyped
action input will only show up at CI runtime otherwise.

### Step 6: open PRs

🚧 [++"nave pen open"++](../reference/cli/pen/open.md) is planned but not yet shipped.
In the meantime, use `gh pr create` inside each repo, or script the loop manually.
