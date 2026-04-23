# Schemas

**TL;DR:** Nave maintains a local cache of JSON Schemas for each tracked file kind,
plus a dynamic validator for GitHub Action `with:` blocks. Schemas let you check
that a config (or a proposed codemod output) is well-formed *before* it hits the
fleet.

## Why schemas

Anti-unification tells you how configs differ. It doesn't tell you whether a proposed
new value is *valid* — for that you need a spec. Most IaC file kinds have one:

- [`pyproject.toml`](https://www.schemastore.org/pyproject.json) (SchemaStore)
- [`dependabot.yml`](https://www.schemastore.org/dependabot-2.0.json) (SchemaStore)
- [GitHub Actions workflows](https://www.schemastore.org/github-action.json) (SchemaStore)
- `Cargo.toml` — [no SchemaStore entry yet](https://github.com/rust-lang/cargo/issues/12883)

Nave pulls these into a local cache at init time and uses them to validate tracked
files and pen transformations.

## Schema IDs

The registry is keyed by `SchemaId`, an enum with variants including:

- `Dependabot`
- `GithubWorkflow`
- `GithubAction`
- `Pyproject`

The mapping from path → schema is hard-coded in `nave_schemas::schema_for_path` and
matches default `tracked_paths`. Custom paths that don't map to a known schema are
still parsed (and therefore `check` covers them), but aren't validated against a schema.

## Enumerated values

Schemas are often richer than their parsed structure. Take the `interval` hole in a
dependabot config: at the type level it's just a string, but the schema constrains
it to an enum:

```json
"schedule-interval": {
  "type": "string",
  "enum": [
    "daily", "weekly", "monthly", "quarterly",
    "semiannually", "yearly", "cron"
  ]
}
```

If a codemod substitutes `"montly"` (typo) into a dependabot config, bare parsing
won't catch it — the value is still a valid string. Schema validation will.

## Action `with:` validation

GitHub Actions are a special case: each action has its own inputs, declared in the
repo's `action.yml`. There's no single schema that covers every `with:` block — you
have to fetch the referenced action at the pinned ref and check the `with:` against
its `inputs` declaration.

`nave schemas validate --check-actions` does exactly this:

1. Parse every workflow in the pen.
2. For each `uses: owner/repo@ref`, fetch `action.yml` at that ref (cached).
3. Check each `with:` key against the action's `inputs`:
   - Missing required inputs → error.
   - Unknown inputs → error.
   - Deprecated inputs → warning with the deprecation message.

This catches a class of bugs that no static schema can: "you upgraded the action
version and one of its inputs got renamed."

Subpath actions (`owner/repo/path@ref`) are currently skipped.

## Validation workflow

```bash
# Pull schemas on first run (also done automatically by `nave init`)
nave schemas pull

# Check a pen's tracked files
nave schemas validate my-pen

# Also check action inputs
nave schemas validate my-pen --check-actions

# CI-style: fail on first error
nave schemas validate my-pen --fail-fast
```

Output is one line per file, with a summary at the end. `--json` gives the structured
form for scripting.

## What schemas don't do

Schema validation is a *structural* check. It doesn't verify semantics ("does this
workflow actually do what you think?") or runtime behaviour ("does the build
succeed?"). For those you still need CI — which is why pen codemods push to branches
and integrate with PR checks. See [Pens](pens.md).
