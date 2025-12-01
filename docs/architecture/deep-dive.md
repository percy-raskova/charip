---
title: Architectural Deep Dive
---

# Architectural Deep Dive & Gap Analysis

This document provides a critical analysis of the discrepancy between the current implementation and the goals outlined for MyST support. It identifies specific architectural gaps that must be filled.

## The Parsing Paradigm Shift

### Current Reality: Regex

The current `Vault` relies on **Lazy Static Regexes** to identify structure.

- **Location**: `src/vault/mod.rs`
- **Mechanism**: `Reference::new()`, `MDHeading::new()`, etc., scan raw text strings
- **Limitation**: Flat and context-agnostic—cannot handle nested structures or MyST's complex block-level constructs

### Target: AST-Based Parsing

```{admonition} Phase 1 Status
:class: tip

Basic MyST parsing via `markdown-rs` is now implemented in `src/myst_parser.rs`. The `scan_for_myst()` visitor extracts directives and anchors.
```

The full transition requires a **Visitor Pattern** implementation:

```rust
// Target flow (simplified)
let ast = markdown::to_mdast(text, &parse_options);
let mut visitor = MystVisitor::new();
visitor.visit(&ast);
```

**Crucial Logic**: The visitor must handle "lifting"—inspecting standard AST nodes (like `CodeBlock`) to detect MyST directives (e.g., `{toctree}`).

## The Concurrency Model

### Current: Shared State with RwLock

```rust
struct Backend {
    vault: Arc<RwLock<Option<Vault>>>,
    // ...
}
```

On `didChange`, the handler acquires a **write lock** on the entire vault.

```{warning}
While `RwLock` allows concurrent reads, a write lock blocks *all* reads. Re-parsing large files could cause editor stutter.
```

### Target: Actor-Based Architecture

```{list-table} Proposed Actor Model
:header-rows: 1

* - Component
  - Responsibility
* - Main Thread (LSP)
  - Handles `textDocument/completion`. Holds a read-only view.
* - Indexer Actor
  - Receives `didChange` events via channel. Parses files. Applies deltas with brief write locks.
```

**Benefit**: The LSP main loop never performs heavy parsing synchronously.

## Graph Data Structure

### Current: Implicit Relationships

Relationships are computed on-demand:

- **Backlinks**: `Vault::select_references_for_referenceable` performs a **parallel linear scan** over all references
- **Complexity**: O(N) where N is total references in the vault
- **Problem**: Acceptable for small vaults, problematic for large documentation sets (5k+ files)

### Target: Explicit petgraph

```{admonition} Dependency Status
:class: note

`petgraph` is in `Cargo.toml` but **unused** in the codebase. Phase 2 will activate it.
```

Implementation requirements:

Bi-directional Map
: `NodeIndex` must map back to `PathBuf` efficiently

Edge Metadata
: Store `Reference` objects on edge weights for "Go to Definition"

Lookup Performance
: Backlinks become O(1) graph traversal: `graph.neighbors_directed(node, Incoming)`

## Configuration: `conf.py` Support

### Current Reality

`src/config.rs` reads `.moxide.toml` and imports settings from `.obsidian/`.

### Target: Sphinx Configuration

To support MyST properly, the LSP must understand:
- Custom directives and roles defined in `conf.py`
- `myst_enable_extensions` settings
- Substitution variables

```{warning}
Without `conf.py` parsing, custom directives will be flagged as errors or fail to autocomplete.
```

**Strategy**: Lightweight regex parser to extract `extensions` and `myst_enable_extensions` from Python config files.

## Summary of Critical Work

```{list-table} Phase 2 Priorities
:header-rows: 1
:widths: 10 50 40

* - Priority
  - Task
  - Location
* - 1
  - Implement `MystWorkspaceGraph` using `petgraph`
  - New `src/graph.rs`
* - 2
  - Refactor `Vault` from HashMap to graph-based storage
  - `src/vault/mod.rs`
* - 3
  - Spawn Indexer Actor for background parsing
  - `src/main.rs`
* - 4
  - Add `conf.py` parsing for MyST configuration
  - `src/config.rs`
```
