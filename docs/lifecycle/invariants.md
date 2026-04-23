# Invariants

The lifecycle has a small set of rules that each command obeys. They exist so that
the system composes cleanly — no hidden network dependencies, no surprise writes.

## The four invariants

### 1. `scan` does not fetch file contents

Scan lists repos and indexes which tracked files exist. It does not clone, sparse
check out, or read any file body. A successful scan leaves the cache in a state
where `pull` can do its job incrementally.

**Why:** Separating "what exists" from "what's in it" makes scans cheap enough to run
frequently. A scan over a 50-repo fleet is seconds.

### 2. `pull` does not analyse

Pull materialises tracked files into the cache. It does not parse them, validate them,
or derive anything from them. If a YAML file is malformed, pull still stores it; that's
`check`'s problem.

**Why:** Keeps pull's error model trivial ("did git succeed?") and lets you inspect
raw files after a problem pull without re-running network operations.

### 3. `search`, `build`, `check`, `schemas` never write

The analysis layer is purely functional over the cache. The same cache + same
invocation = the same output. If you need fresh data, `scan` first.

**Why:** Makes iteration cheap. You can try 30 variations of a `--match` query without
touching anything remote or accidentally mutating state.

### 4. `pen` is the only writer

All writes to the fleet — git commits, pushes, PR operations — happen through the
pen system. No command outside `pen` touches remote state.

**Why:** Writes need transaction scope, freshness checks, and rollback semantics.
Concentrating them into one subsystem lets those concerns live in one place.

## Consequences

These invariants aren't just style guidelines — they're load-bearing:

- **Reproducibility.** An analysis is a pure function of the cache. Re-running
  `build --filter dependabot` tomorrow gives the same output unless scan/pull ran
  in between.
- **Offline analysis.** After `pull`, you can use every analysis command without
  network access. Useful on flights, useful in CI.
- **Auditable writes.** Every change to the fleet has a pen behind it. You can find
  which pen produced which PR, and roll back at any stage.
- **Concurrency story.** Reads can run in parallel freely. Writes are serialised
  per-pen, not per-fleet, because pens are isolated per transaction.

## What's outside the invariants

A few things don't fit the pattern cleanly and are called out explicitly:

- `schemas pull` fetches JSON Schemas over the network. It's part of the validation
  stack but technically a data-layer operation. `nave init` runs it once.
- `pen sync` re-evaluates the filter against the current cache, which is a read-only
  operation, but is scoped to the pen subsystem because that's where its output is used.
- `schemas validate --check-actions` fetches `action.yml` files from remotes. This is
  the one validation command with a network dependency; first-time fetches are cached
  per `owner/repo@ref`.
