# ++"nave pen status"++

Show per-repo state for a pen.

## Usage

```bash
--8<-- "docs/_snippets/cli/pen/status.txt"
```

## Output

One row per repo:

```
lmmx/comrak                     tree=clean    fresh=fresh    run=not-run     up-to-date
lmmx/polars-fastembed           tree=dirty    fresh=fresh    run=run-local   ahead 2
lmmx/page-dewarp                tree=clean    fresh=stale    run=not-run     up-to-date
```

Four state axes per repo:

| Column     | Values                                                |
|------------|-------------------------------------------------------|
| `tree`     | `clean`, `dirty`, `missing`                           |
| `fresh`    | `fresh`, `stale`                                      |
| `run`      | `not-run`, `run-local`, `run-pushed`                  |
| Divergence | `up-to-date`, `ahead N`, `behind N`, `diverged N/M`   |

## Use cases

- **Before `pen exec`** — check nothing's dirty; you'll lose uncommitted work otherwise.
- **After `pen exec`** — confirm expected changes landed.
- **Before planned `pen run`** — verify `stale=0/N` so the codemod operates on the
  right set.
- **In CI** — `--json` + `jq` for programmatic checks.
