# Nave

**Fleet-level operations for OSS package repositories.**

<div align="center">
  <img width="160" height="160" alt="nave logo" src="https://lmmx.github.io/nave/images/logo.png" />
</div>

Nave treats the set of repos a user or org maintains as a single structured dataset
rather than a heap of independent projects. If you're maintaining dozens of packages
— each with its own `pyproject.toml`, GitHub Actions workflows, dependabot config,
and pre-commit hooks — Nave lets you query, diff, and bulk-edit them as a fleet.

## Primitives

- **Fleet** — every repo a user owns, taken together.
- **Cache** — a local sparse-checkout projection of the fleet's tracked files.
- **Template** — the shared skeleton across a set of configs, with variable *holes* where they diverge.
- **Schema** — a JSON Schema layer that validates tracked files (and GitHub Action `with:` blocks) before and after edits.
- **Pen** — a named, ephemeral, per-transaction workspace holding full shallow clones of a filtered subset of the fleet, on which codemods run.

Reads are cheap and happen against the cache. Writes only ever happen in pens.

## Where to start

- **Concepts** — the model, in increasing order of depth: fleet → state → cache → query language → templates → schemas → pens → operations → config.
- **Lifecycle** — how the pieces compose (`init` → `scan` → `pull` → analyse → pen).
- **CLI reference** — every command, every flag.
- **Examples** — drift analysis, lower-bound rollout, Rust-binding surveillance.

## Design background

The motivating design essays are in the [Fleet Ops](https://cog.spin.systems/fleet-ops)
blog series; the docs here are the operational counterpart.
