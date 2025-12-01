---
title: Architecture Overview
---

# Architecture Overview

**{{project}}** is a Rust-based Language Server Protocol (LSP) implementation built using the `tower-lsp` framework. It is designed to run as a standalone binary that communicates with text editors (clients) via standard I/O (stdio).

## Core Components

### Entry Point (`src/main.rs`)

The `main` function parses CLI arguments (using `clap`). It can run in two modes:

CLI Mode
: Runs specific commands like opening a daily note (`cli::run_daily`) or config file (`cli::run_config`).

LSP Mode
: Starts the LSP server using `Server::new(stdin, stdout, socket).serve(service)`.

### Backend Struct

The primary state container for the LSP server:

`client`
: Handle to the LSP client for sending notifications/requests.

`vault`
: `Arc<RwLock<Option<Vault>>>` - The thread-safe, in-memory database of Markdown files.

`opened_files`
: `Arc<RwLock<HashSet<PathBuf>>>` - Tracks files currently open in the editor.

`settings`
: `Arc<RwLock<Option<Settings>>>` - Global configuration.

### The Vault (`src/vault/mod.rs`)

The `Vault` is the central brain of the application—an in-memory graph/database representing the user's notes.

```{list-table} Vault Characteristics
:header-rows: 1

* - Aspect
  - Description
* - Construction
  - Built by walking the directory tree (`WalkDir`), reading `.md` files, and parsing them in parallel using `rayon`.
* - Storage
  - `md_files`: HashMap mapping file paths to `MDFile` structs. `ropes`: HashMap mapping file paths to `ropey::Rope`.
* - Updates
  - On `did_change`, the Vault is updated incrementally. `did_change_watched_files` triggers full reconstruction.
```

## Concurrency Model

```{admonition} Threading Architecture
:class: note

- **Async Runtime**: `tokio` handles the async main loop and LSP request handling.
- **Parallelism**: `rayon` is used for CPU-intensive tasks (parsing, diagnostics, searching).
- **Synchronization**: `Arc<RwLock<...>>` protects shared state with short-duration locks.
```

## Request Lifecycle

1. **LSP Request**: Client sends a request (e.g., `textDocument/completion`).
2. **Handler**: `Backend` trait method is called (e.g., `completion`).
3. **State Access**: The handler acquires a read lock on the `Vault` and `Settings`.
4. **Logic**: The handler delegates to a specific module (e.g., `completion::get_completions`).
5. **Response**: The result is returned to the client.

## Key Crates

| Crate | Purpose |
|-------|---------|
| `tower-lsp` | LSP protocol implementation |
| `tokio` | Async runtime |
| `rayon` | Parallel iteration |
| `ropey` | Efficient text editing (rope data structure) |
| `regex` | Parsing Markdown syntax |
| `nucleo-matcher` | Fuzzy matching for completions |
| `petgraph` | Graph structure (Phase 2) |
