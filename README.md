# nave

Fleet-level modelling and operations for OSS package repos.

<div align="center">
  <img width="200" height="200" alt="nave logo" src="https://github.com/user-attachments/assets/15aaaa82-0300-45ca-a616-8535cf46c416" />
</div>

Rust core (`nave-rs`), Python entry point (`nave`).

## Develop

    cargo test --workspace   # bootstrap husky hooks on first run
    just build
    just run -- --help

## Python

    maturin develop
    python -m nave --help
