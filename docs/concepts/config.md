# Config

All user-level settings live in `~/.config/nave.toml` (`$XDG_CONFIG_HOME`), with
env-var overrides using `NAVE_` prefix and `__` section separator.

[++"nave init"++](../reference/cli/init.md) writes a commented default.

## Layout

```toml
[github]
username = "your-user"  # optional; probed from `gh status` if available
per_page = 100          # GitHub API page size (max 100)
use_gh_cli = true       # probe `gh` for auth/username

[scan]
tracked_paths = [
    "pyproject.toml",
    "Cargo.toml",
    ".pre-commit-config.yaml",
    ".pre-commit-config.yml",
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
    ".github/dependabot.yml",
    ".github/dependabot.yaml",
]
case_insensitive = true

[discovery]
exclude_forks = true
exclude_archived = true

[cache]
root = "/custom/path"   # defaults to XDG cache dir

[pen]
root = "/custom/path"   # defaults to XDG data dir

[schemas]
sources = { dependabot = "https://...", ... }   # override URLs if needed
```

## Resolution order

For each setting, [++"nave"++](../reference/cli/main.md) consults in order:

1. CLI flag (where applicable)
2. Environment variable
3. `~/.config/nave.toml`
4. Baked-in default

[++"nave"++](../reference/cli/main.md) uses [figment2] to load config from this hierarchy of 'providers'.

[figment2]: https://docs.rs/figment2/latest/figment2/

## Environment overrides

Any field can be overridden with an env var. Section separator is `__`
(double underscore). Examples:

```bash
NAVE_GITHUB__USERNAME=foo
NAVE_DISCOVERY__EXCLUDE_FORKS=false
NAVE_SCAN__CASE_INSENSITIVE=true
```

This is handy for CI and for one-off runs without editing the file.


## Auth

- `gh` CLI auth — used if `use_gh_cli = true` and no token is set (recommended).
- `NAVE_GITHUB_TOKEN` — explicit token, overriding the one from `gh auth token`.
- Anonymous — fallback which will hit the 60 req/hr rate limit quickly on first [++"nave scan"++](../reference/cli/scan.md).

## Tracked paths

`tracked_paths` is glob-based with gitignore semantics: `*`, `**`, `?`, `[abc]`,
`{a,b}` all work. Path components are matched relative to each repo's root.

The list is intentionally narrow by default. Broadening it increases scan time
roughly linearly. Narrowing it mid-project requires [++"nave scan --prune"++](../reference/cli/scan.md)
(and removing `~/.cache/nave/meta.toml`) to evict repos that no longer match.

## Logging

Verbose logging via `NAVE_LOG`:

```bash
NAVE_LOG=debug nave scan
NAVE_LOG=trace nave pen run my-pen
```

Follows [tracing-subscriber]'s `EnvFilter` syntax, so per-module filtering works
(`NAVE_LOG=nave_pen=debug,info`).

[tracing-subscriber]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html
