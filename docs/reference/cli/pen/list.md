# `nave pen list`

List pens, optionally filtered by state.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen/list.txt"
```

## Output

Text form, one line per pen:

```
nave/lowest-direct      12 repos  dirty=0/12 stale=0/12 run=0/12
nave/dependabot-weekly   3 repos  dirty=0/3  stale=1/3  run=0/3
```

Each suffix shows counts of repos in a given state.

## Filters

Filters are per-state, per-value. A pen matches a filter if *any* of its repos is in
that state:

```bash
nave pen list -f working-tree=dirty
nave pen list -f freshness=stale -f run-state=run-local
```

Valid values:

| Key            | Values                                      |
|----------------|---------------------------------------------|
| `working-tree` | `clean`, `dirty`, `missing`                 |
| `freshness`    | `fresh`, `stale`                            |
| `run-state`    | `not-run`, `run-local`, `run-pushed`        |

## JSON

`--json` emits the full per-repo state array, one entry per repo per pen. Useful
for piping into `jq`:

```bash
nave pen list --json | \
  jq '.[] | select(.states[] | .working_tree == "dirty") | .name'
```
