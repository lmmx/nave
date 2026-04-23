# ++"nave init"++

Create `~/.config/nave.toml` interactively.

## Usage

```bash
--8<-- "docs/_snippets/cli/init.txt"
```

## What it does

1. Checks for an existing config and prompts before overwriting (unless `--force`).
2. Probes `gh status` for your GitHub username (unless `--no-interaction`, in which
   case it uses whatever `gh` reports or leaves the field blank).
3. Prompts for:
   - whether to use `gh` for auth;
   - `per_page` (clamped to 1–100).
4. Writes a commented config with the default `tracked_paths` list.
5. Pulls the schema cache (non-fatal on network failure — re-run
   [++"nave schemas pull"++](../reference/cli/schemas/pull.md) later if offline).

## Example

```bash
nave init                 # interactive
nave init --no-interaction  # take all defaults
nave init --force         # overwrite existing
```

## The written file

See [Config](../../concepts/config.md) for the full schema. The file is a commented
TOML document; the comments explain glob syntax and which fields are most commonly
edited.
