# Fleet

**TL;DR:** A fleet is the complete set of public, non-fork, non-archived repositories
belonging to a GitHub user or organisation. Nave treats that set as a structured dataset.

## Why a fleet (not a repo)

Most repo tooling operates one repo at a time. That's fine if you maintain one or two.
Once you're maintaining ~10+, you start writing shell loops over the GitHub API because
the unit of interest has shifted: you care about patterns across configs, not the configs
themselves. You ask questions like:

- Which of my repos use `maturin` *and* have pytest in CI?
- How many of my dependabot configs pin `weekly` vs `monthly`?
- Which of my `pyproject.toml`s still pin an old Python lower bound?

These are fleet-level queries. They are awkward-to-impossible to express against the
GitHub API directly, and scripts to simulate them tend to be one-off and lossy.

Nave's position: model the fleet as a first-class dataset, then layer analysis and
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

Private repos are out of scope (supporting them is an explicit non-goal).

## What counts as data

A fleet's dataset is not every file in every repo — that would be both prohibitively
large and mostly noise. Nave tracks a small set of *infrastructure-as-code* files:
the configs that declaratively describe how a repo builds, tests, and releases.

Default `tracked_paths`:

- `pyproject.toml`
- `Cargo.toml`
- `.pre-commit-config.yaml` / `.yml`
- `.github/workflows/*.yml` / `.yaml`
- `.github/dependabot.yml` / `.yaml`

The set is configurable, glob-based (gitignore semantics), and intended to stay narrow.
See [Config](config.md) for the full knob list.

## Fleet vs projection

The fleet lives on GitHub. Nave holds a *projection* of it locally in the cache. The
cache is eventually consistent with the fleet: `nave scan` refreshes it incrementally
based on each repo's `pushed_at` timestamp. See [Cache](cache.md) and [State model](state-model.md).
