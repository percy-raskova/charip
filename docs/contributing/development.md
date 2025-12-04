---
title: Development Setup
---

# Development Setup

This guide helps you set up a development environment for charip-lsp.

## Prerequisites

### Rust Toolchain

Install Rust via rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

charip-lsp requires the **nightly** toolchain:

```bash
rustup toolchain install nightly
rustup default nightly
```

### Editor

Any editor works, but for the best experience developing an LSP:
- Neovim with rust-analyzer
- VS Code with rust-analyzer extension

## Getting the Code

```bash
git clone https://github.com/user/charip-lsp.git
cd charip-lsp
```

## Building

### Debug Build

For development:

```bash
cargo build
```

Binary at `target/debug/charip`.

### Release Build

For testing performance:

```bash
cargo build --release
```

Binary at `target/release/charip`.

### Check Without Building

Fast syntax and type checking:

```bash
cargo check
```

## Running

### As LSP Server

```bash
cargo run
```

The server starts and waits for LSP communication on stdin/stdout.

### CLI Commands

```bash
cargo run -- daily    # Open today's daily note
cargo run -- config   # Open config file
```

## Code Quality

### Formatting

Format code before committing:

```bash
cargo fmt
```

CI checks formatting with `cargo fmt --check`.

### Linting

Run clippy for lint checks:

```bash
cargo clippy
```

Not currently enforced in CI, but recommended.

## Project Structure

```
charip-lsp/
├── src/
│   ├── main.rs           # Entry point, Backend
│   ├── vault/            # Core data structures
│   ├── completion/       # Autocomplete logic
│   ├── myst_parser.rs    # MyST syntax parsing
│   └── ...
├── docs/                 # This documentation (Sphinx/MyST)
├── vscode-extension/     # VS Code integration
├── TestFiles/            # Sample vault for testing
└── ai-docs/              # AI agent reference (YAML)
```

## Development Workflow

### Making Changes

1. Create a branch: `git checkout -b feature/my-feature`
2. Make changes
3. Run tests: `cargo test`
4. Format: `cargo fmt`
5. Commit (see {doc}`commit-philosophy`)
6. Push and create PR

### Testing Your Changes

Point your editor at the debug binary:

```lua
-- Neovim example
configs.charip = {
  default_config = {
    cmd = { '/path/to/charip-lsp/target/debug/charip' },
    -- ...
  }
}
```

Then open a file in `TestFiles/` to test.

### Debugging

Add logging:

```rust
tracing::info!("Processing file: {:?}", path);
```

Run with logging enabled:

```bash
MOXIDE_LOG=debug cargo run
```

## VS Code Extension

The VS Code extension is in `vscode-extension/`:

```bash
cd vscode-extension
npm install
npm run compile
```

For development:

```bash
npm run watch  # Rebuild on changes
```

Press F5 in VS Code to launch an Extension Development Host.

## Documentation

Docs are built with Sphinx:

```bash
cd docs
pip install -r requirements.txt
make html
```

Open `docs/_build/html/index.html` in a browser.

## Continuous Integration

CI runs on every push and PR:

1. `cargo build --verbose --locked`
2. `cargo test --verbose`

Ensure these pass locally before pushing.
