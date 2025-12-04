---
title: Installation
---

# Installation

charip-lsp is distributed as a single binary. Choose your installation method below.

## From Source (Recommended)

Building from source ensures you have the latest features.

### Requirements

- Rust nightly toolchain
- Git

### Steps

```bash
# Clone the repository
git clone https://github.com/user/charip-lsp.git
cd charip-lsp

# Build release binary
cargo build --release

# The binary is at target/release/charip
```

### Install to PATH

```bash
# Option 1: Copy to a directory in your PATH
sudo cp target/release/charip /usr/local/bin/

# Option 2: Add target/release to your PATH
export PATH="$PATH:/path/to/charip-lsp/target/release"
```

## Verify Installation

```bash
charip --version
```

You should see version information printed.

## Pre-built Binaries

Pre-built binaries for common platforms will be available in future releases.

| Platform | Status |
|----------|--------|
| Linux x86_64 | Coming soon |
| macOS ARM64 | Coming soon |
| macOS x86_64 | Coming soon |
| Windows x64 | Coming soon |

## Updating

To update to the latest version:

```bash
cd charip-lsp
git pull
cargo build --release
```

## Troubleshooting

### Rust Nightly Required

charip-lsp requires the Rust nightly toolchain:

```bash
rustup toolchain install nightly
rustup default nightly
```

Or build with:

```bash
cargo +nightly build --release
```

### Build Fails with Missing Dependencies

On Debian/Ubuntu, you may need:

```bash
sudo apt install build-essential pkg-config libssl-dev
```

## Next Steps

With charip-lsp installed, proceed to {doc}`editor-setup` to configure your editor.
