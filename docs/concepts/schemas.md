# Schemas

Nave maintains a local cache of JSON Schemas for each tracked file kind,
plus a dynamic validator for GitHub Action `with:` blocks.

Schemas let you check that a config (or a proposed codemod output) is
well-formed *before* it writing to a pen.

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

Configs are often more well-typed than their deserialised structure types alone,
they typically have well-defined schemas.

Take the `interval` hole in a dependabot config: at the type level it's just a string,
but the schema constrains it to an enum:

```json
"schedule-interval": {
  "type": "string",
  "enum": [
    "daily", "weekly", "monthly", "quarterly", "semiannually", "yearly", "cron"
  ]
}
```

If a codemod substitutes `"monthly"` into a dependabot config, parsing alone
won't catch it, the value is still a valid string, but schema validation will.

## Action `with:` validation

GitHub Actions are a special case: each action has its own inputs, declared in the
repo's `action.yml`. There's no single schema that covers every `with:` block — you
have to fetch the referenced action at the pinned ref and check the `with:` against
its `inputs` declaration.

[++"nave schemas validate --check-actions"++](../reference/cli/schemas.md)
does exactly this:

1. Parse every workflow in the pen.
2. For each `uses: owner/repo@ref`, fetch `action.yml` at that revision (which is cached).
3. Check each `with:` key against the action's `inputs`, invalidate any missing required
   inputs or unknown inputs, and warn for deprecated inputs.

This catches a common class of bugs that static schema would let through, upgrading an action
version and one of its inputs got renamed. CI tests can catch them but it's better
not to wait for the rubber to hit the road.

Subpath actions (`owner/repo/path@ref`) are currently not supported and are skipped.

## Validation workflow

```bash
# Pull schemas on first run
nave schemas pull

# Check a pen's tracked files
nave schemas validate my-pen

# Also check action inputs
nave schemas validate my-pen --check-actions

# CI-style: fail on first error
nave schemas validate my-pen --fail-fast
```

The output shows one line per repo in the pen, with a summary at the end
of failures per file. As with many [++"nave"++](../reference/cli/nave.md) commands the `--json` flag gives this
in a structured form for machine reading.

## What schemas don't do

Schema validation is a *structural* check. It doesn't verify semantics ("does this
workflow actually do what you think?") or runtime behaviour ("does the build
succeed?"). For those you still need CI, which is why pen codemods push to branches
to integrate with PR checks. See [Pens](pens.md).
