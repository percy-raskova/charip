---
title: Testing & Development
---

# Testing & Development Environment

This document outlines the testing strategies, development workflows, and CI/CD pipeline.

## Testing Strategy

The project employs a multi-layered testing approach, combining unit tests for core logic with integration tests for the VS Code extension.

### Unit Tests (Rust)

Location
: Co-located with source code in `src/` using `#[cfg(test)]` modules

Coverage areas:
- **Parsing Logic**: `src/vault/mod.rs`, `src/vault/parsing.rs`
- **Configuration**: `src/config.rs`
- **Commands**: `src/commands.rs` (date resolution, file paths)
- **MyST Parser**: `src/myst_parser.rs` (directives, anchors)
- **CLI**: `src/cli.rs`

```bash
# Run all tests
cargo test

# Run specific test
cargo test myst_integration

# Run with output
cargo test -- --nocapture
```

### Integration/E2E Tests (VS Code Extension)

Location
: `vscode-extension/client/src/test/`

Mechanism
: Standard VS Code extension testing framework (`@vscode/test-electron`)

```bash
cd vscode-extension
npm run test
```

### Test Data

The `TestFiles/` directory contains a sample vault for testing:

- Daily notes (e.g., `2024-03-17.md`)
- Standard notes (`Test.md`)
- Configuration files (`.moxide.toml`, `.obsidian/`)
- Subdirectories (`folder/`, `test/`)

## Development Environment

### Rust Toolchain

```{important}
The CI configuration specifies the **nightly** toolchain.

```bash
rustup toolchain install nightly
rustup default nightly
```
```

Required components:
- `rustfmt` for style checks

### VS Code Extension Development

```bash
cd vscode-extension
npm install
npm run compile
```

## CI/CD Pipeline

The project uses GitHub Actions (`.github/workflows/rust.yml`).

```{list-table} CI Steps
:header-rows: 1

* - Step
  - Command
* - Checkout
  - `actions/checkout@v4`
* - Install Toolchain
  - nightly Rust with `rustfmt`
* - Format Check
  - `cargo fmt --check`
* - Build
  - `cargo build --verbose --locked`
* - Test
  - `cargo test --verbose`
```

Trigger
: Pushes and Pull Requests to `main` branch

Platform
: `ubuntu-latest`

## Development Tips

```{tip}
When developing the LSP core, point your local VS Code extension to the debug binary for a tight feedback loop:

1. Build debug binary: `cargo build`
2. Configure VS Code extension to use `target/debug/markdown-oxide`
3. Use "Developer: Reload Window" to test changes
```
