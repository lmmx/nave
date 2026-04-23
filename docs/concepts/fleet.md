# Fleet

A fleet is the complete set of public, non-fork, non-archived repositories
belonging to the GitHub user or organisation.

Nave treats a fleet as a first-class primitive, and is designed around it.

## Why a fleet (not a repo)

Most repo tooling operates one repo at a time, which becomes unwieldy for those maintaining many.
At that point, you start writing shell loops over the GitHub API because
you care about patterns across configs, not the configs themselves.

You will likely find yourself with questions like:

- Which of my repos use `maturin` *and* have pytest in CI?
- How many of my dependabot configs pin `weekly` vs `monthly`?
- Which of my `pyproject.toml`s still pin an old Python lower bound?

These are fleet-level queries. They range from awkward to inhibiting to express against the
GitHub API directly, and scripts to simulate them tend to be one-off and lossy.

Nave models the fleet as a first-class primitive, then layer analysis and
mutation on top of that model.

## What's in the fleet

By default, `nave scan` calls `GET /users/{username}/repos`, which returns only **public**
repos even when authenticated. Forks and archived repos are filtered out. This is
configurable in `~/.config/nave.toml`:

```toml
[discovery]
exclude_forks = true
exclude_archived = true
```

Private repos are out of scope (supporting them is an explicit non-goal),
as is use with multiple users/organisations at once

## What fleet data is tracked

A fleet's tracked file set is a small minority of the repo — primarily the config files
for package management and CI/CD (*infrastructure-as-code*/IaC):
the configs that declaratively describe how a repo builds, tests, and releases.

Default `tracked_paths`:

- `pyproject.toml`
- `Cargo.toml`
- `.pre-commit-config.yaml` / `.yml`
- `.github/workflows/*.yml` / `.yaml`
- `.github/dependabot.yml` / `.yaml`

The set is configurable, glob-based (gitignore semantics), and deliberately narrow.
See [Config](config.md) for the full list.

## Fleet vs projection

The fleet lives on GitHub and is read-only.

Nave holds a *projection* of it locally in the cache.
The cache is eventually consistent with the fleet:
`nave scan` refreshes it incrementally based on each repo's `pushed_at` timestamp.

See [Cache](cache.md) and [Core Primitives](primitives.md).
