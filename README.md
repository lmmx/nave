# nave

Fleet-level modelling and operations for OSS package repos.

<div align="center">
  <img width="200" height="200" alt="nave logo" src="https://github.com/user-attachments/assets/15aaaa82-0300-45ca-a616-8535cf46c416" />
</div>

Rust core (`nave-rs`), Python entry point (`nave`).

## Motivation

See blog post: [Fleet Ops](https://cog.spin.systems/fleet-ops).

## Try it

Run the CLI (still in development) like so:

```bash
cargo run --bin nave -- init --no-interaction
cat ~/.config/nave.toml
# You should see the commented header followed by [github], [cache], [discovery]
# with the full tracked_paths list explicit in the file.

cargo run --bin nave -- discover
ls ~/.cache/nave/repos/
# See the metadata that got recorded for user's repos

cargo run --bin nave -- fetch
ls ~/.cache/nave/repos/
# See the repo files that got fetched (only tracked file types)

NAVE_LOG=debug cargo run --bin nave -- fetch
# Run this to see details of what gets fetched
```
