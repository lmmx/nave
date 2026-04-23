# ++"nave schemas"++

Manage the JSON Schema cache and validate tracked files.

## Usage

```bash
--8<-- "docs/_snippets/cli/schemas.txt"
```

Schemas are pulled from their canonical locations by default:

| Schema            | Source                                                                       |
|-------------------|------------------------------------------------------------------------------|
| `dependabot`      | `https://www.schemastore.org/dependabot-2.0.json`                            |
| `pyproject`       | `https://www.schemastore.org/pyproject.json`                                 |
| `github-workflow` | `https://www.schemastore.org/github-workflow.json`                           |
| `github-action`   | `https://www.schemastore.org/github-action.json`                             |

You can override these in `~/.config/nave.toml`:

```toml
[schemas.sources]
dependabot = "https://example.com/my-dependabot-schema.json"
```

Cargo.toml has no SchemaStore entry yet (tracked in
[rust-lang/cargo#12883](https://github.com/rust-lang/cargo/issues/12883)); it's
parsed by `check` but not schema-validated.

## Subcommands

- [++"nave schemas pull"++](schemas/pull.md)
- [++"nave schemas list"++](schemas/list.md)
- [++"nave schemas validate"++](schemas/validate.md)
