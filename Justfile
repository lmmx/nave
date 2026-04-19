set shell := ["bash", "-cu"]

default:
    @just --list

# Build the Rust workspace
build:
    cargo build --workspace

# Release build
release:
    cargo build --workspace --release

# Run the binary
run *ARGS:
    cargo run --bin nave -- {{ARGS}}

# Build the Python wheel via maturin
wheel:
    maturin build --release

# Install into the current venv for smoke-testing the Python entry point
develop:
    maturin develop

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Format Rust + Python
fmt:
    cargo fmt --all
    ruff format python/

# Lint Rust + Python
lint:
    just clippy
    ruff check python/

# Tests
test:
    cargo nextest run --no-fail-fast

# Pre-commit: fast stuff only — fmt check + clippy + python lint
pre-commit:
    just fmt
    cargo fmt --all -- --check
    just clippy
    ruff check python/
    ruff format --check python/

# Pre-push: also run the test suite
pre-push:
    just pre-commit
    just test
