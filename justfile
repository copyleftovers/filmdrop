# filmdrop development commands
# Run `just` or `just --list` to see all available commands

# Default recipe - show available commands
default:
    @just --list

# Check all crates compile
check:
    cargo check --workspace

# Format all code
fmt:
    cargo fmt --all

# Lint all crates
clippy:
    cargo clippy --workspace

# Run all tests
test:
    cargo test --workspace

# Build release binaries
build:
    cargo build --release

# Run the web server (reads .env for GALLERY_BUCKET and AWS vars)
run:
    #!/usr/bin/env bash
    set -e
    if [ -f .env ]; then
        set -a
        source .env
        set +a
    fi
    cargo run --bin filmdrop-web

# Upload images via CLI (usage: just upload "Album Name" /path/to/photos/)
upload NAME PATH:
    cargo run --bin filmdrop upload --name "{{NAME}}" {{PATH}}

# Install the CLI tool
install:
    cargo install --path filmdrop-cli
