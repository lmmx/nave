# ++"nave"++

Top-level entrypoint. Dispatches to subcommands.

## Usage

```bash
--8<-- "docs/_snippets/cli/nave.txt"
```

## Environment variables

| Variable              | Effect                                                |
|-----------------------|-------------------------------------------------------|
| `NAVE_LOG`            | `tracing-subscriber` filter (e.g. `debug`, `trace`)   |
| `NAVE_GITHUB_TOKEN`   | Personal access token for the GitHub API              |
| `NAVE_<SECTION>__<KEY>` | Override any config field (e.g. `NAVE_GITHUB__USERNAME`) |

See [Config](../../concepts/config.md) for the full list.
