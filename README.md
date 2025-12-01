<p align="center">
  <img src="https://img.shields.io/badge/rust-1.70+-orange?style=flat-square&logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/LSP-3.17-blue?style=flat-square" alt="LSP">
  <img src="https://img.shields.io/badge/MyST-Markdown-red?style=flat-square" alt="MyST">
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License">
</p>

<h1 align="center">
  <br>
  🇰🇵
  <br>
  <code>자립</code> charip-lsp
  <br>
</h1>

<h4 align="center">A Language Server for <a href="https://myst-parser.readthedocs.io/">MyST Markdown</a> — Build Your Own Infrastructure</h4>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#usage">Usage</a> •
  <a href="#philosophy">Philosophy</a>
</p>

---

## What is charip-lsp?

**charip-lsp** provides IDE intelligence for [MyST (Markedly Structured Text)](https://myst-parser.readthedocs.io/) documents — the extended Markdown syntax used by Sphinx documentation systems.

Forked from [markdown-oxide](https://github.com/Feel-ix-343/markdown-oxide), this LSP targets technical documentation and knowledge management workflows built on MyST/Sphinx rather than Obsidian/Logseq PKM systems.

```
┌─────────────────────────────────────────────────────────────┐
│  {ref}`my-section`  →  (my-section)=                        │
│                        # My Section                         │
│                                                             │
│  ```{note}            ┌─────────────────┐                   │
│  Content              │ Autocomplete    │                   │
│  ```                  │ ► note          │                   │
│                       │   warning       │                   │
│                       │   tip           │                   │
│                       └─────────────────┘                   │
└─────────────────────────────────────────────────────────────┘
```

## Features

### MyST Role Support
- **Extraction** — `{ref}`, `{doc}`, `{term}`, `{numref}`, `{eq}`, `{download}`, `{abbr}`
- **Go-to-definition** — Jump from `{ref}`target`` to its anchor
- **Find references** — See all places that reference an anchor

### MyST Directive Autocomplete
Type `` ```{ `` and get completions for:
- **Admonitions** — `note`, `warning`, `tip`, `hint`, `danger`, `caution`...
- **Structure** — `toctree`, `include`, `glossary`
- **Media** — `figure`, `image`, `code-block`
- **Tables** — `table`, `list-table`, `csv-table`

### Role Target Autocomplete
Type `{ref}`` ` and get completions for:
- MyST anchors (`(my-target)=`)
- Document headings (slugified)
- Relative document paths (for `{doc}`)

### Inherited from markdown-oxide
- Markdown link intelligence
- Heading navigation
- Footnote support
- Tag completion

## Installation

```bash
# From source
cargo install --locked --path .

# Or build locally
cargo build --release
# Binary at target/release/markdown-oxide
```

## Usage

Configure your editor to use `markdown-oxide` as the language server for `.md` files.

<details>
<summary><strong>Neovim (nvim-lspconfig)</strong></summary>

```lua
require("lspconfig").markdown_oxide.setup({
    capabilities = vim.tbl_deep_extend('force',
        require("cmp_nvim_lsp").default_capabilities(),
        { workspace = { didChangeWatchedFiles = { dynamicRegistration = true } } }
    ),
})
```
</details>

<details>
<summary><strong>VS Code</strong></summary>

Use the [Markdown Oxide extension](https://marketplace.visualstudio.com/items?itemName=FelixZeller.markdown-oxide) and point it to your local binary.
</details>

<details>
<summary><strong>Helix</strong></summary>

Install the binary to your path — Helix auto-detects `markdown-oxide`.
</details>

## Philosophy

**Charip** (자립, *ja-rip*) means "self-reliance" in Korean — one of the three pillars of the Juche idea alongside *chaju* (political independence) and *chawi* (self-defense).

> True independence requires building one's own infrastructure rather than depending on external systems.

This project embodies that principle:
1. MyST/Sphinx documentation deserves first-class editor support
2. Technical writers shouldn't have to choose between rich markup and IDE features
3. Revolutionary movements need revolutionary infrastructure — build your own tools

## Status

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | MyST role extraction | ✅ Complete |
| 2 | Anchor resolution | ✅ Complete |
| 2.5 | Directive labels | ✅ Complete |
| 3 | Directive autocomplete | ✅ Complete |
| 4 | Role target autocomplete | ✅ Complete |

**149 tests passing**

## Development

```bash
cargo build          # Debug build
cargo test           # Run tests
cargo fmt            # Format code
cargo clippy         # Lint
```

## License

MIT — See [LICENSE](LICENSE) for details.

---

<p align="center">
  <sub>Built for those who write their own tools.</sub>
</p>
