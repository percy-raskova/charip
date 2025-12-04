---
title: Architecture
---

# Architecture

This document explains how charip-lsp is structured and how its components work together.

## High-Level Overview

```
┌─────────────────────────────────────────────────────────┐
│                        Editor                           │
│                   (Neovim, VS Code)                     │
└─────────────────────────┬───────────────────────────────┘
                          │ LSP Protocol (JSON-RPC)
                          ▼
┌─────────────────────────────────────────────────────────┐
│                       Backend                            │
│                    (src/main.rs)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
│  │   Client    │  │   Vault     │  │    Settings     │  │
│  │   Handle    │  │  (Graph)    │  │                 │  │
│  └─────────────┘  └─────────────┘  └─────────────────┘  │
└─────────────────────────┬───────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        ▼                 ▼                 ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│  Completions  │ │   Go-to-Def   │ │  Diagnostics  │
│               │ │  References   │ │               │
└───────────────┘ └───────────────┘ └───────────────┘
```

## Core Components

### Backend (`src/main.rs`)

The `Backend` struct is the LSP server's state container:

```rust
struct Backend {
    client: tower_lsp::Client,
    vault: Arc<RwLock<Option<Vault>>>,
    settings: Arc<RwLock<Option<Settings>>>,
    opened_files: Arc<RwLock<HashSet<PathBuf>>>,
}
```

It implements the `LanguageServer` trait, handling all LSP requests.

### Vault (`src/vault/mod.rs`)

The vault is the in-memory representation of the documentation project:

```rust
pub struct Vault {
    pub ropes: MyHashMap<Rope>,
    root_dir: PathBuf,
    pub graph: VaultGraph,
    pub node_index: HashMap<PathBuf, NodeIndex>,
}
```

Key responsibilities:
- Parse and store all markdown files
- Maintain the document graph
- Provide query methods for LSP features

### DocumentNode (`src/vault/graph.rs`)

Each file becomes a node in the graph:

```rust
pub struct DocumentNode {
    pub path: PathBuf,
    pub references: Vec<Reference>,
    pub headings: Vec<MDHeading>,
    pub myst_symbols: Vec<MystSymbol>,
    pub glossary_terms: Vec<GlossaryTerm>,
    // ... other fields
}
```

## Module Organization

```
src/
├── main.rs              # LSP server, Backend struct
├── vault/
│   ├── mod.rs           # Vault struct, queries
│   ├── graph.rs         # DocumentNode, EdgeKind, graph ops
│   ├── ast_refs.rs      # AST-based parsing
│   ├── types.rs         # MDHeading, MDTag, etc.
│   └── helpers.rs       # Path resolution
├── completion/
│   ├── mod.rs           # Completion dispatcher
│   ├── myst_directive_completer.rs
│   ├── myst_role_completer.rs
│   └── ...
├── myst_parser.rs       # MyST syntax extraction
├── gotodef.rs           # Go to definition
├── references.rs        # Find references
├── diagnostics.rs       # Error detection
├── hover.rs             # Hover information
├── rename.rs            # Symbol renaming
└── config.rs            # Settings loading
```

## Request Flow

### Example: Go to Definition

1. Editor sends `textDocument/definition` with cursor position
2. `Backend::goto_definition` acquires read lock on vault
3. Delegates to `gotodef::goto_definition`
4. Function finds the reference at cursor position
5. Looks up target in graph
6. Returns target location to editor

### Example: Find References

1. Editor sends `textDocument/references` with cursor position
2. `Backend::references` acquires read lock on vault
3. Identifies what's under the cursor (anchor, heading, etc.)
4. Queries graph for incoming edges (backlinks)
5. Returns all reference locations

## Concurrency Model

### Async Runtime

`tokio` handles the LSP event loop:

```rust
#[tokio::main]
async fn main() {
    // LSP server runs on tokio runtime
}
```

### Parallel Processing

`rayon` parallelizes CPU-intensive work:

- Initial vault construction (parsing all files)
- Computing diagnostics across files
- Searching for symbols

### Synchronization

Shared state uses `Arc<RwLock<T>>`:

- Read locks for queries (concurrent)
- Write locks for updates (exclusive)
- Locks held for minimal duration

## Update Strategy

### On File Change (`did_change`)

1. Acquire write lock on vault
2. Update the rope for that file
3. Re-parse the file into a DocumentNode
4. Update graph edges
5. Release lock

### On External Change (`did_change_watched_files`)

For file moves/deletes, the vault is fully reconstructed. This is expensive but rare.

## Key Design Decisions

### Graph vs. HashMap

Early versions used `HashMap<PathBuf, MDFile>`. The graph provides:
- O(1) backlink queries (vs O(n) scan)
- Structural analysis (cycles, orphans)
- Foundation for future features (toctree visualization)

### AST-Based Parsing

MyST directives appear as code blocks to standard Markdown parsers. charip-lsp uses `markdown-rs` to get an AST, then inspects code blocks for directive syntax.

### Unified Reference Model

All reference types (markdown links, MyST roles, tags) use the same `Reference` enum. This enables consistent handling across the codebase.
