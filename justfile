# Check formatting and linting
check:
    cargo fmt --check
    cargo clippy --workspace -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Run test suite (includes all compiled-in languages for test coverage)
test:
    cargo test --workspace --features codequery-cli/test-all-langs

# Run full test suite including ignored/LSP tests
test-all:
    cargo test --workspace --features codequery-cli/test-all-langs
    cargo test --workspace --features codequery-cli/test-all-langs -- --ignored

# Debug build
build:
    cargo build --workspace

# Release build
release:
    cargo build --workspace --release

# Run cq with arguments
run *ARGS:
    cargo run --package codequery-cli -- {{ARGS}}

# Run cq-mcp server
run-mcp:
    cargo run --package codequery-mcp

# Full CI pipeline
ci: check test build
    cargo doc --workspace --no-deps

# Build and open docs
doc:
    cargo doc --workspace --no-deps --open

# Run smoke tests against real open-source projects
smoke-test *LANG:
    bash scripts/smoke-test.sh {{LANG}}

# Validate stack graph results against LSP ground truth
lsp-validate:
    bash scripts/lsp-validation.sh

# Generate man page (writes to cq.1, or `just man install` to install)
man *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --bin generate-manpage --quiet
    if [ "{{ARGS}}" = "install" ]; then
        cargo run --bin generate-manpage --quiet -- /usr/local/share/man/man1/cq.1
        echo "installed to /usr/local/share/man/man1/cq.1"
    else
        cargo run --bin generate-manpage --quiet -- cq.1
        echo "generated cq.1"
    fi

# Clean build artifacts
clean:
    cargo clean

# Coverage report
coverage:
    cargo tarpaulin --workspace --out html
