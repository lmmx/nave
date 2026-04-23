# `nave check`

Verify that tracked configuration files parse and round-trip cleanly.

## Usage

```
Check tracked configs parse and round-trip cleanly

Usage: nave check [OPTIONS]

Options:
      --json           Emit results as JSON instead of text
      --failures-only  Only print failures (skip rows marked `ok`).
                       Text mode only.
  -h, --help           Print help
```

## What it does

For every tracked file in the cache:

1. Parse (YAML or TOML, chosen by extension).
2. Re-serialise.
3. Parse the re-serialised output.
4. Compare the two parsed ASTs.

Each file lands in one of these outcomes:

| Outcome          | Meaning                                              |
|------------------|------------------------------------------------------|
| `ok`             | Parses, round-trips, no drift                        |
| `drift`          | Re-serialised form parses but differs from original  |
| `parse_failed`   | Input doesn't parse (malformed file on disk)         |
| `render_failed`  | Parsed AST won't re-serialise                        |
| `reparse_failed` | Re-serialised output doesn't parse                   |
| `unknown_format` | File extension not recognised                        |
| `missing`        | File in index but not on disk                        |

## Exit code

Non-zero if any of `drift`, `parse_failed`, `render_failed`, `reparse_failed` occur.
Useful in CI.

## Why run it

Two reasons:

- **Sanity check before codemods.** If a file doesn't round-trip cleanly today, it
  won't after a codemod either — you want to know in advance.
- **Find hand-edited configs.** YAML and TOML both have multiple valid
  representations; files that drift often indicate hand-edits that the canonical
  form would flatten. Depending on your policy, that's either fine or worth fixing.

## Output

```bash
nave check --failures-only
```

```
         drift  lmmx/polars-fastembed  pyproject.toml  [toml]  — reordered keys
  parse_failed  lmmx/my-broken-repo    .github/dependabot.yml  [yaml]  — expected key
── summary ──
          ok  47
       drift  1
parse_failed  1
```
