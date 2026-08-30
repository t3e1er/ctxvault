# Justfile — local developer workflow commands.
# Install just: cargo install just
# Usage: just <recipe>

# Default recipe: build + test
default: check

# ── Quality ──────────────────────────────────────────

# Run all quality checks (what CI runs)
ci: fmt-check clippy test deny docs

# Type-check without building (fast feedback)
check:
    cargo check --workspace --all-features --all-targets

# Run clippy lints
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Check formatting without changing files
fmt-check:
    cargo fmt --all -- --check

# Auto-format all code
fmt:
    cargo fmt --all

# ── Build ────────────────────────────────────────────

# Build all crates (debug)
build:
    cargo build --workspace --all-features

# Build in release mode
build-release:
    cargo build --workspace --all-features --release

# ── Test ─────────────────────────────────────────────

# Run all tests
test:
    cargo test --workspace --all-features

# Run tests with output (for debugging)
test-verbose:
    cargo test --workspace --all-features -- --nocapture

# Run a specific test by name
test-one NAME:
    cargo test --workspace --all-features -- {{NAME}} --nocapture

# ── Security ─────────────────────────────────────────

# Run cargo-deny (license + vulnerability + source checks)
deny:
    cargo deny check

# Run cargo-audit (vulnerability scan)
audit:
    cargo audit

# Configure local git repository to use .githooks
setup-hooks:
    git config core.hooksPath .githooks

# Show dependency tree (useful for inspecting transitive deps)
deps:
    cargo tree --workspace

# Show duplicate dependencies
deps-dupes:
    cargo tree --workspace --duplicates

# ── Documentation ────────────────────────────────────

# Build documentation
docs:
    cargo doc --workspace --all-features --no-deps

# Build and open documentation
docs-open:
    cargo doc --workspace --all-features --no-deps --open

# ── Maintenance ──────────────────────────────────────

# Update Cargo.lock (respects pinned versions in Cargo.toml)
update:
    cargo update

# Clean all build artifacts
clean:
    cargo clean

# Check MSRV compatibility
msrv:
    cargo +1.80 check --workspace --all-features

# ── Dev helpers ──────────────────────────────────────

# Watch for changes and re-check (requires cargo-watch)
watch:
    cargo watch -x 'check --workspace --all-features'

# Run the CLI binary
run *ARGS:
    cargo run --bin cxtvault -- {{ARGS}}
