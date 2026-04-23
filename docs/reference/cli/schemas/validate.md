# ++"nave schemas validate"++

Validate tracked files in a pen against their schemas.

## Usage

```bash
--8<-- "docs/_snippets/cli/schemas/validate.txt"
````

## Description

Validates every tracked file in the specified pen against its schema.

```bash
nave schemas validate <PEN>
```

## Behaviour

* Validates all tracked files in the pen
* Outputs per-repo results
* Supports structured output via `--json`

## Failure types

* **Schema errors** — file does not conform to JSON Schema
* **Action errors** (with `--check-actions`) — invalid `with:` inputs in workflows

## Options

* `--check-actions` — validate GitHub Actions inputs against upstream definitions
* `--fail-fast` — stop on first failure
* `--json` — emit machine-readable output

Exit code is non-zero if any validation fails.
