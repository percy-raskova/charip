# Getting Started

(installation-anchor)=
## Installation

Install the LSP server using cargo:

```bash
cargo install charip
```

This section can be referenced with {ref}`installation-anchor`.

^setup-instructions

## Quick Start

After installation, configure your editor. See {doc}`configuration` for details.

The {term}`LSP` communicates over stdio by default.

### Editor Support

Supported editors include:

- Neovim (via nvim-lspconfig)
- VS Code (via extension)
- Helix (native support)

^editor-list

## Next Steps

- Read the {doc}`../reference/api` documentation
- Learn about {term}`MyST` syntax
- Configure {ref}`config-anchor` options

## Footnotes

The LSP uses the tower-lsp[^1] crate for protocol handling.

Parsing is done with markdown-rs[^2] for CommonMark compliance.

[^1]: tower-lsp is an async LSP framework for Rust.

[^2]: markdown-rs provides fast, spec-compliant Markdown parsing.

## Tags

#tutorial #getting-started #installation
