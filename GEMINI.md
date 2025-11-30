# markdown-oxide

## Project Overview
**markdown-oxide** is a high-performance, Rust-based Language Server Protocol (LSP) implementation for Markdown. It is specifically designed for **Personal Knowledge Management (PKM)** workflows, taking inspiration from tools like Obsidian and Logseq.

It allows users to bring their own text editor (VS Code, Neovim, Emacs, etc.) and gain "IDE-like" features for their notes, such as:
*   **Wikilink support:** `[[Link]]` navigation and management.
*   **Cross-reference resolution:** Go to Definition for headers, blocks, and tags.
*   **Daily Notes:** Quick access to daily journal entries.
*   **Completions:** Autocomplete for links, tags, and specialized syntax.
*   **Diagnostics:** Detect broken links and other issues.

The project is currently extending its capabilities to support **MyST (Markedly Structured Text)**, aiming to bridge the gap between casual note-taking and rigorous technical documentation.

## Architecture
*   **Language:** Rust (2021 edition).
*   **LSP Framework:** `tower-lsp`.
*   **Async Runtime:** `tokio`.
*   **Data Structure:** A "Vault" (in-memory graph) representing the relationships between Markdown files.
    *   **Parsing:** Uses a combination of `markdown-rs` (planned/partial) and custom Regex-based parsing for Obsidian-specific syntax.
    *   **Graph:** `petgraph` (used/planned) to model the document mesh.
    *   **Parallelism:** `rayon` is used for parallel indexing of the vault.
*   **Text Handling:** `ropey` for efficient text manipulation and line/column mapping.

## Building and Running

### Prerequisites
*   Rust Toolchain (`cargo`, `rustc`)
*   Node.js & npm (for the VS Code extension)

### Build the LSP Server
To build the standalone binary:
```bash
cargo build --release
```
The binary will be located at `target/release/markdown-oxide`.

### Run the LSP Server
The server typically runs over `stdio` when spawned by an editor client.
```bash
cargo run --release
```

### CLI Commands
The binary also supports CLI commands for standalone operations (e.g., managing daily notes).
```bash
# Example: Open/Create today's daily note
cargo run --release -- daily
```

### VS Code Extension
The `vscode-extension/` directory contains the client-side code for VS Code.
```bash
cd vscode-extension
npm install
npm run compile
# To package:
# vsce package
```

## Key Files & Directories

*   **`src/main.rs`**: The entry point. Sets up the `tower-lsp` service, handles the main event loop, and dispatching LSP requests (`initialize`, `didOpen`, etc.).
*   **`src/vault/`**: The core "brain" of the application.
    *   `mod.rs`: Defines the `Vault` struct and methods for constructing/updating the file index.
    *   `parsing.rs`: Logic for parsing Markdown files into the internal object model.
*   **`src/completion/`**: Logic for providing autocomplete suggestions.
*   **`src/gotodef/`**: Logic for "Go to Definition" (resolving links to files, headers, or blocks).
*   **`PLAN.md`**: A detailed architectural roadmap for future features, specifically MyST support and Neovim integration.
*   **`Cargo.toml`**: Rust dependencies and metadata.

## Development Conventions

*   **Style:** Standard Rust formatting (`cargo fmt`).
*   **Testing:** Unit tests are co-located with code (e.g., `mod tests` inside source files).
*   **Error Handling:** Extensive use of `Result` and `Option` to ensure safety.
*   **Concurrency:** Heavy use of async/await (`tokio`) for I/O-bound tasks and `rayon` for CPU-bound indexing tasks.
