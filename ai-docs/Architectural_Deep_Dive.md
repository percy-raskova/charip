# Architectural Deep Dive & Gap Analysis

This document provides a critical analysis of the discrepancy between the current implementation (as of `src/`) and the aggressive goals outlined in `PLAN.md`. It identifies specific architectural voids that must be filled to achieve the "MyST Language Server" vision.

## 1. The Parsing Paradigm Shift: Regex vs. AST

### Current Reality
The current `Vault` relies entirely on **Lazy Static Regexes** to identify structure.
*   **File:** `src/vault/mod.rs`
*   **Mechanism:** `Reference::new()`, `MDHeading::new()`, etc., scan the raw text string (derived from `Rope`).
*   **Limitation:** This is flat and context-agnostic. It cannot handle nested structures (e.g., a reference inside a directive inside a list) or MyST's complex block-level constructs.

### The Plan's Requirement
`PLAN.md` mandates a move to `markdown-rs` (AST-based).
*   **The Gap:** The `markdown-rs` crate is present in `Cargo.toml` but **unused** in the codebase.
*   **Implementation Detail:** We need a **Visitor Pattern** implementation.
    *   Instead of `regex.captures_iter(text)`, the new flow will be:
        1.  `let ast = markdown::to_mdast(text, &parse_options);`
        2.  `let mut visitor = MystVisitor::new(db_handle);`
        3.  `visitor.visit(ast);`
    *   **Crucial Logic:** The visitor must handle "lifting". Standard AST nodes (like `CodeBlock`) must be inspected to see if they are actually MyST directives (e.g., `{toctree}`) and processed accordingly.

## 2. The Concurrency Model: `RwLock` vs. Actor

### Current Reality
The `Backend` in `src/main.rs` uses a shared state model:
```rust
struct Backend {
    vault: Arc<RwLock<Option<Vault>>>,
    ...
}
```
*   **Flow:** On `didChange`, the handler calls `bind_vault_mut`, obtaining a **write lock** on the entire vault to update the file.
*   **Bottleneck:** While `RwLock` allows concurrent reads, a write lock blocks *all* reads. Re-parsing a large file or updating graph edges could cause noticeable stutter in the editor (completions blocking).

### The Plan's Requirement
Section 3.3 of `PLAN.md` calls for an **Actor-based architecture**.
*   **The Gap:** There is no channel-based communication or background worker thread currently implemented.
*   **Proposed Architecture:**
    *   **Main Thread (LSP):** Handles `textDocument/completion`. Holds a `Reader` handle to the graph.
    *   **Indexer Actor (Background):** Recieves `didChange` events via a `tokio::sync::mpsc` channel.
        *   Parses the file.
        *   Calculates the *delta* (nodes/edges to add/remove).
        *   Acquires the write lock *briefly* to apply the delta.
    *   **Benefit:** The LSP main loop never performs the heavy parsing/diffing logic synchronously.

## 3. Graph Data Structure: Implicit vs. Explicit

### Current Reality
Relationships are implicit and computed on-demand.
*   **Backlinks:** `Vault::select_references_for_referenceable` performs a **parallel linear scan** (`par_iter`) over *all* references in the vault to find matches.
*   **Performance:** O(N) where N is total references in the vault. Acceptable for small vaults, fatal for large documentation sets (5k+ files).

### The Plan's Requirement
Explicit `petgraph::MultiDiGraph`.
*   **The Gap:** `petgraph` is a dependency but unused.
*   **Implementation Detail:**
    *   **Nodes:** `NodeIndex` needs to map back to `PathBuf` efficiently (bi-directional map).
    *   **Edges:** Need to store "Edge Metadata" (the `Reference` object) on the edge weights to support "Go to Definition" resolution.
    *   **Lookup:** Backlinks become an O(1) (or O(K) neighbors) graph traversal: `graph.neighbors_directed(node, Incoming)`.

## 4. Configuration Strategy: `conf.py`

### Current Reality
`src/config.rs` reads `.moxide.toml` and imports rudimentary settings from `.obsidian/`.

### The Plan's Requirement
To support MyST properly, the LSP must understand the project's specific directives and roles, which are often defined in Sphinx's `conf.py`.
*   **The Gap:** No Python parsing or Sphinx configuration logic exists.
*   **Risk:** Without this, custom directives will be flagged as errors or fail to autocomplete.
*   **Strategy:** Need a lightweight parser (or regex heuristic) to extract `extensions` and `myst_enable_extensions` from `conf.py`.

## 5. External Editor Integration

### Current Reality
The project is purely a Rust binary.

### The Plan's Requirement
Section 6 & 7 describe Lua code and Tree-sitter queries.
*   **The Gap:** These artifacts (Lua scripts, `.scm` query files) do not have a home in the current directory structure.
*   **Action:** Needs a repository restructuring to include a `plugin/` or `contrib/` directory for:
    *   `queries/markdown/injections.scm` (Tree-sitter)
    *   `lua/markdown-oxide/telescope.lua` (Pickers)

## Summary of Critical Work
1.  **Refactor `main.rs`** to spawn an Indexer Actor.
2.  **Implement `MystWorkspaceGraph`** using `petgraph` to replace the `HashMap` vault.
3.  **Write the `MystVisitor`** to utilize `markdown-rs`.
4.  **Create a `contrib/` folder** for Neovim-specific glue code.
