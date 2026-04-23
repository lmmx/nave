import ".just/ship.just"
import ".just/docs_links.just"
import ".just/docs_nav.just"
import ".just/docs_snippets.just"

set shell := ["bash", "-cu"]

default:
    @just --list

# Build the Rust workspace
build:
    cargo build --workspace

# Release build
build-release:
    cargo build --workspace --release

# Check the Rust workspace
check:
    cargo check --workspace

# Run the binary
run *ARGS:
    cargo run --bin nave -- {{ARGS}}

# Build the Python wheel via maturin
wheel:
    maturin build --release --uv

# Install into the current venv for smoke-testing the Python entry point
develop:
    maturin develop --uv

# Install into the current venv for smoke-testing the Python entry point
develop-release:
    maturin develop --release --uv

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

# Full docs refresh (snippets + build)
docs:
    just snippets
    zensical build --clean

build-docs:
    uv run --with zensical --no-project zensical build

check-docs: check-docs-nav check-docs-snippets check-docs-links

# Pre-commit: fast stuff only — fmt check + clippy + python lint
pre-commit:
    just fmt
    cargo fmt --all -- --check
    just clippy
    ruff check python/
    ruff format --check python/
    just check-docs

# Pre-push: also run the test suite
pre-push:
    just pre-commit
    just test
