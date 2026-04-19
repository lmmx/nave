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
cargo run --bin nave -- discover
cargo run --bin nave -- fetch
ls ~/.cache/nave/repos/lmmx/nave/checkout/
# pyproject.toml
NAVE_LOG=debug cargo run --bin nave -- fetch   # second run: should be "updated"
```
