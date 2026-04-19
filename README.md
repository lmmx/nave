# nave

Fleet-level modelling and operations for OSS package repos.
Rust core (`nave-rs`), Python entry point (`nave`).

## Develop

    cargo test --workspace   # bootstrap husky hooks on first run
    just build
    just run -- --help

## Python

    maturin develop
    python -m nave --help