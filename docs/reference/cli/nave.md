# `nave`

Top-level entrypoint. Dispatches to subcommands.

## Usage

```
Fleet ops for OSS package repos

Usage: nave <COMMAND>

Commands:
  init     Interactively create `~/.config/nave.toml`
  scan     List a user's repos and cache the set of tracked files
  pull    Sparse-checkout scanned repos into the cache
  check    Check tracked configs parse and round-trip cleanly
  build    Simplify configs across repos into shared templates
  schemas  Manage the JSON Schema cache and validate tracked files
  search   Search cached repos for substring patterns across tracked files
  pen      Operations on pens (named subsets of the fleet)
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Environment variables

| Variable              | Effect                                                |
|-----------------------|-------------------------------------------------------|
| `NAVE_LOG`            | `tracing-subscriber` filter (e.g. `debug`, `trace`)   |
| `NAVE_GITHUB_TOKEN`   | Personal access token for the GitHub API              |
| `NAVE_<SECTION>__<KEY>` | Override any config field (e.g. `NAVE_GITHUB__USERNAME`) |

See [Config](../../concepts/config.md) for the full list.
