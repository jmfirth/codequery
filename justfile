# Check formatting and linting
check:
    cargo fmt --check
    cargo clippy --workspace -- -D warnings

# Run fast test suite (<10s)
test:
    cargo test --workspace

# Run full test suite including ignored tests
test-all:
    cargo test --workspace
    cargo test --workspace -- --ignored

# Debug build
build:
    cargo build --workspace

# Release build
release:
    cargo build --workspace --release

# Run cq with arguments
start *ARGS:
    cargo run --package codequery-cli -- {{ARGS}}

# Full CI pipeline
ci: check test-all
    cargo doc --workspace --no-deps

# Build and open docs
doc:
    cargo doc --workspace --no-deps --open

# Clean build artifacts
clean:
    cargo clean

# Coverage report
coverage:
    cargo tarpaulin --workspace --out html
