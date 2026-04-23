# `nave pen show`

Show a single pen's details.

## Usage

```
Show a single pen's details

Usage: nave pen show [OPTIONS] [NAME]

Arguments:
  [NAME]  Pen name, or empty when `--filter` is used [default: ]

Options:
      --filter <FILTER>  Regex over pen names. Must match exactly one pen.
      --json             Emit JSON instead of text
  -h, --help             Print help
```

## Output

```
name: nave/lowest-direct
branch: nave/lowest-direct
created: 2026-04-20T14:03:22Z
filter: ["workflow:pytest", "workflow:uv"]
repos (12):
  lmmx/comrak         branch=main  synced=2026-04-20T14:03:25Z
  lmmx/polars-fastembed  branch=main  synced=2026-04-20T14:03:28Z
  ...
```

## `--filter` vs `NAME`

The positional `NAME` is the pen's exact name. `--filter` is a regex that must match
exactly one pen. Useful when pen names are long or you don't remember the exact form:

```bash
nave pen show --filter lowest-direct
```

## JSON

`--json` emits the full pen manifest — filter, repos, branches, sync timestamps.
Same structure as `pen.toml` on disk.
