# nave

[![uv](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json)](https://github.com/astral-sh/uv)
[![PyPI](https://img.shields.io/pypi/v/nave.svg)](https://pypi.org/project/nave)
[![Supported Python versions](https://img.shields.io/pypi/pyversions/nave.svg)](https://pypi.org/project/nave)
[![License](https://img.shields.io/pypi/l/nave.svg)](https://pypi.python.org/pypi/nave)
[![pre-commit.ci status](https://results.pre-commit.ci/badge/github/lmmx/nave/master.svg)](https://results.pre-commit.ci/latest/github/lmmx/nave/master)

**Fleet-level operations for OSS package repos.**

If you maintain multiple repos, each with its own `pyproject.toml`, CI workflows, dependabot config,
pre-commit hooks, and so on, `nave` lets you query and manage these as a _fleet_.

Examples of questions `nave` is built to answer:

- Which of my repos use `maturin` *and* have pytest in CI?
- What's the shared skeleton across all my dependabot configs, and where do they diverge?
- Which repos still pin an old Python version in `pyproject.toml`?

If you have one repo, or a monorepo, this isn't for you. If you have a sprawl of
related-but-drifting configs and you've written shell loops over the GitHub API to
keep track of them, read on.

<div align="center">
  <img width="160" height="160" alt="nave logo" src="https://github.com/user-attachments/assets/15aaaa82-0300-45ca-a616-8535cf46c416" />
</div>

Nave is built in Rust (`nave-rs`) with a Python package as a command line entry point (`nave`).
Background on the design is in the [Fleet Ops](https://cog.spin.systems/fleet-ops) blog series.

## Install

```bash
pip install nave      # or: uv tool install nave
```

You'll also want the [`gh` CLI](https://cli.github.com/) authenticated, or a `NAVE_GITHUB_TOKEN` in your environment.
Anonymous access works but hits the 60 req/hr rate limit quickly on first `nave scan`.

## What it does

### Structural simplification of configs

`nave build` finds the shared skeleton across all tracked configs of the same kind and shows you which fields vary,
how often, and with what values. It's a way to see drift, and to work out which fields are worth standardising.

```bash
nave build --filter dependabot
```

```yaml
━━ .github/dependabot.yml ━━
  instances: 9

  template:
    updates:
      - cooldown?: ⟨?0⟩
        directory: "/"
        package-ecosystem: ⟨?1⟩
        schedule:
          interval: ⟨?2⟩
    version: 2

  holes:
    updates[0].cooldown  [optionalkey]  3/9 optional  [constant when present]
        3× {"default-days":7}
    updates[0].package-ecosystem  [string]  9/9
        8× "github-actions"
        1× "cargo"
    updates[0].schedule.interval  [string]  9/9
        6× "weekly"
        3× "monthly"
```

This is read as: across 9 dependabot configs, they all have the same shape; the ecosystem and interval vary,
and 3 of the 9 set a cooldown. Under the hood this is anti-unification over the parsed YAML/TOML trees,
but you don't need to care about that to use it.

JSON output is available with `--json` for scripting.

### Search across tracked files

`nave search` looks for patterns across your cache of tracked configs.

Plain terms match substrings anywhere; `workflow:` scopes a term to CI workflow files.

```bash
nave search maturin workflow:pytest
```

```
lmmx/comrak
lmmx/polars-fastembed
lmmx/page-dewarp
lmmx/polars-luxical
```

To see *where* in each file a term matched — particularly useful for `pyproject.toml`
where you often want to know which field, not just which file:

```bash
nave search maturin workflow:pytest --output holes | rg -v workflows
```

```
pyproject.toml  build-system.build-backend  (2 hits)
pyproject.toml  build-system.requires[0]    (2 hits)
pyproject.toml  dependency-groups.build[0]  (2 hits)
pyproject.toml  dependency-groups.dev[0]    (2 hits)
pyproject.toml  tool.maturin                (2 hits)
```

Other useful flags: `--explain` (show matched files and terms), `--json`, `--count`,
`--sort pushed-at --limit N` (most recently touched first).

## Setup commands

Three plumbing commands you'll run in order on first use:

```bash
nave init            # write ~/.config/nave.toml (one-shot)
nave scan        # enumerate repos and index tracked files
nave pull           # sparse-checkout tracked files into ~/.cache/nave/
```

By default, `scan` only looks at repos that have changed since the previous `scan`.

To re-examine every repo (e.g. after narrowing `tracked_paths`, or to remove cached repos that no longer match),
delete `~/.cache/nave/meta.toml` and use `nave scan --prune`.

There's also `nave check`, which verifies that every tracked config parses without errors.

Verbose logging: `NAVE_LOG=debug nave <cmd>`.

## Configuration

All settings live in `~/.config/nave.toml`. `nave init` writes a commented default
you can edit. The knob most people will want is `tracked_paths`:

```toml
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
exclude_forks = true
```

Glob semantics follow gitignore syntax for `*`, `**`, `?` and `[abc]`.

Any field can be overridden via env var using double-underscore as the section separator:
`NAVE_GITHUB__USERNAME=foo`, `NAVE_DISCOVERY__EXCLUDE_FORKS=false`.

## Scope and privacy

- `nave scan` queries `GET /users/{username}/repos`, which returns only **public** repos even when authenticated.
- Forks and archived repos are filtered out by default. To configure this, edit the user-level config which is written on `nave init`
  or by setting the corresponding environment variables.
- Private repos aren't included (supporting them is a non-goal).

## Architecture

A Rust workspace split across four concerns:

- **CLI & shim** — `nave` (binary, subcommand routing) and a thin `maturin`-packaged
  Python entry point that execs the Rust binary (same pattern as `uv` and `ruff`).
- **Config & cache** — `nave_config` handles layered config via
  [figment2](https://crates.io/crates/figment2), cache layout, and path matching.
- **GitHub I/O** — `nave_github` (REST client with auth probing), `nave_scan`
  (repo listing and tree walking), `nave_pull` (sparse checkout).
- **Modelling** — `nave_parse` (YAML/TOML de/serialisation), `nave_check`
  (WIP), `nave_build` (anti-unification to find minimal template groupings).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for dev setup, the `just` task list, and
git hooks.
