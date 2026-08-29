# Contributing to ctxvault

Thank you for your interest in contributing to **ctxvault**! We welcome bug reports, feature requests, documentation improvements, and pull requests.

This document outlines the workflow and quality standards for contributing.

---

## Code of Conduct

All contributors and maintainers are expected to adhere to our [Code of Conduct](CODE_OF_CONDUCT.md). Please report any unacceptable behavior to the project maintainers.

---

## Development Prerequisites

- **Rust**: 1.80 or later (managed via `rustup`).
- **Rust Components**: `rustfmt`, `clippy`.
- **Optional Tools**:
  - [`just`](https://github.com/casey/just) for running standard developer recipes.
  - [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) for dependency and license audits.

---

## Branching Model & Workflow

We follow a standard fork-and-pull-request workflow:

1. **Fork** the repository on GitHub.
2. **Clone** your fork locally:
   ```bash
   git clone git@github.com:<your-username>/ctxvault.git
   cd ctxvault
   ```
3. **Create a branch** using our naming conventions:
   - `feature/<short-description>`: New features or capabilities
   - `fix/<short-description>`: Bug fixes
   - `docs/<short-description>`: Documentation changes
   - `chore/<short-description>`: Refactoring, dependencies, or maintenance
4. **Make your changes** cleanly with atomic commits.
5. **Run the quality gates** locally before pushing (see below).
6. **Push** to your fork and open a Pull Request against `main` (or `master`).

---

## Quality Gates & Verification

All Pull Requests must pass our automated CI suite. You can run all checks locally using `just` or standard `cargo` commands:

### Using `just` (Recommended)

```bash
just check       # Fast type checking
just fmt-check   # Check formatting
just fmt         # Auto-format all files
just clippy      # Run clippy lints with -D warnings
just test        # Run unit, integration, and e2e tests
just deny        # Verify license and dependency safety
just ci          # Run full CI test suite locally
```

### Using `cargo` directly

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo check --workspace --all-features
cargo doc --workspace --all-features --no-deps
```

---

## Architectural & Code Guidelines

- **Zero Unsafe Code**: The entire workspace enforces `unsafe_code = "forbid"`. Any PR introducing `unsafe` code blocks will be rejected.
- **Pure Rust Dependencies**: Avoid introducing external C/C++ dependencies or bindings unless strictly necessary and vetted.
- **Error Handling**: Use `thiserror` for library-level errors in `cxtvault-common`/`cxtvault-core` and `anyhow` for top-level application orchestration in `cxtvault-cli`.
- **Documentation**: All public APIs, structs, and functions must be documented (`missing_docs = "warn"` is enforced).

---

## Submitting Pull Requests

- Use the provided [Pull Request Template](.github/pull_request_template.md).
- Clearly describe the purpose, implementation details, and verification steps.
- Ensure all CI status checks pass.
- Repository maintainers will review your PR and squash-merge it upon approval.
