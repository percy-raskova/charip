# Architecture Overview

**Markdown-Oxide** is a Rust-based Language Server Protocol (LSP) implementation built using the `tower-lsp` framework. It is designed to run as a standalone binary that communicates with text editors (clients) via standard I/O (stdio).

## Core Components

### 1. Entry Point (`src/main.rs`)
*   **`main` function**: Parses CLI arguments (using `clap`). It can run in two modes:
    *   **CLI Mode**: Runs specific commands like opening a daily note (`cli::run_daily`) or config file (`cli::run_config`).
    *   **LSP Mode**: Starts the LSP server using `Server::new(stdin, stdout, socket).serve(service)`.
*   **`Backend` Struct**: The primary state container for the LSP server.
    *   `client`: Handle to the LSP client for sending notifications/requests.
    *   `vault`: `Arc<RwLock<Option<Vault>>>` - The thread-safe, in-memory database of Markdown files.
    *   `opened_files`: `Arc<RwLock<HashSet<PathBuf>>>` - Tracks files currently open in the editor.
    *   `settings`: `Arc<RwLock<Option<Settings>>>` - Global configuration.

### 2. The Vault (`src/vault/mod.rs`)
The `Vault` is the central brain of the application. It is an in-memory graph/database representing the user's notes.
*   **Construction**: Built by walking the directory tree (`WalkDir`), reading `.md` files, and parsing them in parallel using `rayon`.
*   **Storage**:
    *   `md_files`: A HashMap mapping file paths to `MDFile` structs (parsed metadata/structure).
    *   `ropes`: A HashMap mapping file paths to `ropey::Rope` (efficient text manipulation structures).
*   **Updates**: When a file changes (`did_change`), the `Vault` is updated incrementally. `did_change_watched_files` triggers a full reconstruction (e.g., external file moves).

### 3. Concurrency Model
*   **Async Runtime**: `tokio` is used for the async main loop and LSP request handling.
*   **Parallelism**: `rayon` is used heavily for CPU-intensive tasks like:
    *   Parsing all files during initialization.
    *   Computing diagnostics across the entire vault.
    *   Searching for references.
*   **Synchronization**: `Arc<RwLock<...>>` protects shared state (`Vault`, `Settings`). The code frequently uses a pattern of binding read/write locks for short durations to perform operations.

## Request Lifecycle
1.  **LSP Request**: Client sends a request (e.g., `textDocument/completion`).
2.  **Handler**: `Backend` trait method is called (e.g., `completion`).
3.  **State Access**: The handler acquires a read lock on the `Vault` and `Settings`.
4.  **Logic**: The handler delegates to a specific module (e.g., `completion::get_completions`) passing the `Vault` reference.
5.  **Response**: The result is returned to the client.

## Key Crates
*   **`tower-lsp`**: LSP protocol implementation.
*   **`tokio`**: Async runtime.
*   **`rayon`**: Parallel iteration.
*   **`ropey`**: Efficient text editing (CRDT-like structure for text).
*   **`regex`**: Heavy reliance on Regex for parsing Markdown syntax (Links, Tags, Headings).
*   **`nucleo-matcher`**: Fuzzy matching for completions.
