# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**charip-lsp** is a fork of [markdown-oxide](https://github.com/Feel-ix-343/markdown-oxide) being adapted to support **MyST (Markedly Structured Text)** documents. The goal is to provide LSP intelligence for a personal knowledge vault (`~/rstnotes`) that publishes to a Sphinx website.

### Target Environment (`~/rstnotes`)

The LSP is designed for a specific MyST/Sphinx environment:
- **Build system**: Sphinx with `myst-parser`
- **MyST extensions**: `colon_fence`, `deflist`, `dollarmath`, `substitution`, `tasklist`
- **Custom roles**: `{term}`, `{ref}`, `{doc}`, `{tag}`
- **Custom directives**: `definition`, `honeypot`, `ai_content` (in `_extensions/`)
- **Frontmatter schema**: Strict YAML with `zkid`, `category`, `tags`, `publish`, `status`

The original markdown-oxide was designed for Obsidian/Logseq PKM workflows with wikilinks.

## Build & Test Commands

```bash
# Build
cargo build --release          # Release build (binary at target/release/markdown-oxide)
cargo build                    # Debug build

# Test
cargo test                     # Run all unit tests
cargo test <test_name>         # Run specific test
cargo test --lib               # Run library tests only

# Format & Lint
cargo fmt                      # Format code
cargo fmt --check              # Check formatting (used in CI)
cargo clippy                   # Lint (not in CI but recommended)

# Run LSP server (communicates via stdio)
cargo run --release

# CLI commands
cargo run --release -- daily   # Open/create today's daily note
cargo run --release -- config  # Open config file
```

**CI Requirements**: Uses **nightly** Rust toolchain. Ensure you have `rustup toolchain install nightly` and formatting passes before pushing.

## Architecture

### Core Components

1. **Backend** (`src/main.rs`): The LSP server state container using `tower-lsp`. Holds:
   - `vault: Arc<RwLock<Option<Vault>>>` - in-memory document graph
   - `settings: Arc<RwLock<Option<Settings>>>` - configuration
   - `opened_files: Arc<RwLock<HashSet<PathBuf>>>` - editor-opened files

2. **Vault** (`src/vault/mod.rs`): Central data structure representing the knowledge graph:
   - `md_files: HashMap<PathBuf, MDFile>` - parsed file structures
   - `ropes: HashMap<PathBuf, Rope>` - raw text content (via `ropey`)
   - Built by walking directory tree and parsing `.md` files in parallel (`rayon`)

3. **MDFile** (`src/vault/mod.rs`): Parsed representation of a single Markdown file containing:
   - `references: Vec<Reference>` - outgoing links (wikilinks, markdown links, tags)
   - `headings`, `indexed_blocks`, `tags`, `footnotes`, `codeblocks`

4. **Reference vs Referenceable**: Core abstraction for link resolution:
   - `Reference` = source of a link (what points)
   - `Referenceable` = target of a link (what can be pointed to)

### Key Modules

| Module | Purpose |
|--------|---------|
| `src/completion/` | Autocomplete (links, tags, callouts, footnotes) |
| `src/gotodef.rs` | Go-to-definition for wikilinks/headers |
| `src/references.rs` | Find all references (backlinks) |
| `src/rename.rs` | Rename symbols across vault |
| `src/diagnostics.rs` | Broken link detection |
| `src/myst_parser.rs` | MyST directive parsing (Phase 1 complete) |
| `src/config.rs` | Settings loading including Obsidian import |
| `src/daily.rs` | Daily notes functionality |

### Concurrency Model

- **Async**: `tokio` for LSP request handling
- **Parallelism**: `rayon` for CPU-intensive vault indexing
- **Synchronization**: `Arc<RwLock<...>>` for shared state

### Parsing Strategy

Currently **regex-based** parsing for Obsidian syntax. The MyST transition (in progress) will move to **AST-based** parsing via `markdown-rs`:

| Current | Target (MyST) |
|---------|---------------|
| Regex patterns for links/headings | `markdown-rs` AST traversal |
| HashMap storage | `petgraph` graph structure |
| Linear reference lookup | Graph neighbor queries |

## Active Development: MyST Support

The project is implementing MyST support in phases:

1. **Phase 1 (Complete)**: Parser rewrite - `myst_parser.rs` handles MyST directives via `markdown-rs`
2. **Phase 2 (Planned)**: Graph architecture using `petgraph` for toctree/include relationships
3. **Phase 3 (Planned)**: LSP capabilities for directive completion, reference resolution

Key files for MyST work:
- `src/myst_parser.rs` - directive parsing logic
- `PLAN.md` - detailed architectural roadmap (Gemini-generated)
- `ai-docs/MyST_Implementation_Plan.md` - migration checklist
- `ai-docs/Target_Environment_Analysis.md` - rstnotes-specific requirements

## VS Code Extension

Located in `vscode-extension/`:

```bash
cd vscode-extension
npm install
npm run compile
npm run test    # E2E tests
```

## Test Data

`TestFiles/` contains a sample vault for manual testing with daily notes, nested folders, and configuration files.

## Key Dependencies

- `tower-lsp`: LSP protocol implementation
- `markdown`: CommonMark parser (markdown-rs/micromark)
- `ropey`: Efficient text manipulation for position mapping
- `rayon`: Parallel iteration for vault indexing
- `petgraph`: Graph library (planned for MyST graph structure)
- `nucleo-matcher`: Fuzzy matching for completions
