# ++"nave schemas pull"++

Populate the schema cache based on tracked paths.

## Usage

```bash
--8<-- "docs/_snippets/cli/schemas/pull.txt"
```

## Description

Fetches JSON Schemas for each schema kind that applies to the current
`tracked_paths`.

* Network-dependent
* Failures are logged but non-fatal (so that `nave init` can complete offline)

Use `--refresh` to force re-fetching even if schemas are already cached.
