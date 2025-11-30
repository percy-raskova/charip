# MyST Implementation Roadmap

This document outlines the technical plan to extend **Markdown-Oxide** to support **MyST (Markedly Structured Text)**, shifting from a regex-based PKM tool to a graph-based Language Server for technical documentation.

**Based on:** `PLAN.md` and current codebase analysis.

## 1. Current State vs. Target State

| Feature | Current Architecture (`src/vault/`) | Target Architecture (MyST Support) |
| :--- | :--- | :--- |
| **Parsing** | **Regex-based**. `MDFile::new` uses lazy static regexes for links, headings, tags. | **AST-based**. `markdown-rs` (micromark) to generate a concrete syntax tree. |
| **Data Structure** | **HashMap**. `Vault` holds `HashMap<PathBuf, MDFile>`. No explicit graph. | **MultiDiGraph**. `petgraph` to model Toctrees, References, and Includes. |
| **References** | **Flat List**. `vec![Reference]` in `MDFile`. Lookup via linear scan/filter. | **Graph Edges**. `EdgeKind::Reference` in `MystWorkspaceGraph`. |
| **Validation** | **Local/None**. No cycle detection for includes. | **Graph Algorithms**. Cycle detection for `{include}` directives using `petgraph`. |

## 2. Phase 1: The Parser Rewrite

The current parsing logic in `src/vault/mod.rs` and `src/vault/parsing.rs` is rigid. It must be replaced to support MyST directives (nested parsing).

### Impacted Files
*   `src/vault/mod.rs`: The `MDFile::new` constructor and `Reference::new` iterator.
*   `src/vault/parsing.rs`: Currently handles code blocks. Will be replaced by the AST visitor.

### Implementation Strategy
1.  **Introduce `markdown-rs`**: Add as dependency (already done?).
2.  **Create `MystVisitor`**: Implement a visitor pattern to traverse the AST produced by `markdown-rs`.
    *   **Nodes to Handle**: `CodeBlock` (check info string for directives like `{toctree}`), `Link`, `Heading`.
3.  **Refactor `MDFile`**: Instead of storing `Vec<Reference>`, it might need to store a `NodeIndex` or just raw data that the Graph Builder consumes.

## 3. Phase 2: The Graph Architecture

Although `petgraph` is in `Cargo.toml`, it is **not currently used** in the source code. The new architecture will introduce it as the primary data structure.

### Proposed Structures (from PLAN.md)

```rust
use petgraph::graph::{DiGraph, NodeIndex};

pub struct DocumentNode {
    pub uri: String,
    pub title: String,
    pub is_root: bool,
    pub has_targets: Vec<String>,
}

pub enum EdgeKind {
    Reference,                 // [[Link]] or {ref}
    Structure { caption: Option<String> }, // {toctree}
    Transclusion { range: Range }, // {include}
}

pub type MystWorkspaceGraph = DiGraph<DocumentNode, EdgeKind>;
```

### Integration Point
*   **`Vault` Struct**: The `Vault` struct in `src/vault/mod.rs` will likely change from:
    ```rust
    pub struct Vault {
        pub md_files: MyHashMap<MDFile>,
        ...
    }
    ```
    to:
    ```rust
    pub struct Vault {
        pub graph: MystWorkspaceGraph,
        pub file_map: HashMap<PathBuf, NodeIndex>, // Fast lookup for graph nodes
        ...
    }
    ```

## 4. Feature Upgrades

### Completions (`src/completion/`)
*   **Directives**: New completer for ````{name}` syntax.
*   **Roles**: New completer for `{role}` syntax.
*   **References**: `LinkCompleter` will query the `petgraph` for nodes with matching targets/anchors.

### Go to Definition (`src/gotodef.rs`)
*   **Logic Change**: Instead of `select_referenceables_for_reference` (linear scan), query the graph neighbors.
    *   *Input*: File URI + Cursor Position -> Edge.
    *   *Output*: Target Node URI.

### Diagnostics (`src/diagnostics.rs`)
*   **Cycle Detection**: On `didSave`, run `petgraph::algo::is_cyclic_directed` on the inclusion subgraph.
*   **Orphan Check**: Find nodes with 0 in-degree (excluding root).

## 5. Migration Checklist

- [ ] **Prototype Parser**: Create a small binary to test `markdown-rs` parsing of MyST directives.
- [ ] **Graph Prototype**: Implement the `MystWorkspaceGraph` struct and test basic insertions.
- [ ] **Vault Refactor**: Replace `HashMap` storage with Graph storage.
- [ ] **LSP Adapters**: Rewrite `get_completions` and `goto_definition` to use the Graph API.

## 6. Progress Log

### Completed
*   **Phase 1 (Partial)**:
    *   Swapped `markdown-rs` dependency for `markdown` (micromark).
    *   Created `src/myst_parser.rs` module.
    *   Implemented basic AST traversal to identify MyST Directives (e.g., ````{name}`) from fenced code blocks.
    *   Added unit tests for directive parsing.

### Immediate Next Steps
1.  **Support MyST Roles**:
    *   Extend `myst_parser.rs` to identify inline roles (`{role}content`).
    *   Likely requires a regex pass on `Node::Text` nodes since standard CommonMark parsers treat roles as plain text.
2.  **Graph Integration (Phase 2 Start)**:
    *   Create a new module `src/graph.rs` (or similar).
    *   Define `MystWorkspaceGraph` using `petgraph`.
    *   Test inserting nodes and edges.

