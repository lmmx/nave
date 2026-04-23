# `nave scan`

Enumerate a user's repositories and index tracked configuration files.

## Usage

```
List a user's repos and cache the set of tracked files

Usage: nave scan [OPTIONS]

Options:
      --user <USER>     Override the GitHub username for this run
      --no-interaction  Don't prompt; fail fast if anything is missing
      --prune           Remove cached repo directories that no longer match
                        the current user's repos or scan filters (forks,
                        archived, missing tracked files, etc). Only
                        effective on a full (non-incremental) run.
  -h, --help            Print help
```

## What it does

1. Resolves the GitHub username (CLI flag > config > `gh status` > prompt).
2. Calls `GET /users/{username}/repos`, filtering forks and archived repos.
3. For each repo, walks the tree to identify files matching `tracked_paths`.
4. Writes a metadata index to the cache at `~/.cache/nave/meta.toml`.

## Scope

- **Only public repos.** The `GET /users/{username}/repos` endpoint returns public
  repos only, even with authentication. Private repos are out of scope.
- **No file bodies.** Scan indexes file *existence*; `nave pull` fetches the bodies.

## Incrementality

By default, scan only re-examines repos whose `pushed_at` is newer than the most
recent scan. To force a full rescan:

```bash
rm ~/.cache/nave/meta.toml
nave scan --prune
```

`--prune` evicts cached repos that no longer satisfy the filters (e.g. after
narrowing `tracked_paths`).

## Persistence

If you pass `--user` and the config has no username set, the username is saved to
`~/.config/nave.toml` for next time. This is non-fatal on write failure.

## Auth modes

| Mode       | Rate limit   | Source                            |
|------------|--------------|-----------------------------------|
| Token      | 5000 req/hr  | `NAVE_GITHUB_TOKEN`               |
| `gh` CLI   | 5000 req/hr  | `gh` auth, probed if `use_gh_cli` |
| Anonymous  | 60 req/hr    | Fallback; slow on first scan      |
