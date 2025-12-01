---
title: MyST Implementation Roadmap
---

# MyST Implementation Roadmap

This document outlines the technical plan to extend {{project}} to support **MyST (Markedly Structured Text)**, shifting from a regex-based PKM tool to a graph-based Language Server for technical documentation.

## Current State vs. Target State

```{list-table}
:header-rows: 1
:widths: 20 40 40

* - Feature
  - Current Architecture
  - Target Architecture
* - **Parsing**
  - Regex-based. `MDFile::new` uses lazy static regexes.
  - AST-based. `markdown-rs` for concrete syntax tree.
* - **Data Structure**
  - HashMap. `Vault` holds `HashMap<PathBuf, MDFile>`.
  - MultiDiGraph. `petgraph` to model relationships.
* - **References**
  - Flat list. `Vec<Reference>` in `MDFile`.
  - Graph edges. `EdgeKind::Reference` in workspace graph.
* - **Validation**
  - Local/None. No cycle detection.
  - Graph algorithms. Cycle detection via `petgraph`.
```

## Phase 1: The Parser Rewrite

```{admonition} Status: Complete
:class: tip

Phase 1 is complete. MyST directive and anchor parsing is integrated into the vault.
```

### Completed Work

- Swapped to `markdown` crate (micromark) for AST parsing
- Created `src/myst_parser.rs` module
- Implemented `MystSymbolKind` enum (`Directive`, `Anchor`, `Reference`)
- Added `select_myst_symbols()` query method to `Vault`
- Full TDD cycle with integration tests

### Impacted Files

| File | Changes |
|------|---------|
| `src/myst_parser.rs` | New parser using `markdown-rs` |
| `src/vault/mod.rs` | Integration with `myst_symbols` field |
| `src/config.rs` | Added `Settings::default()` for tests |

## Phase 2: The Graph Architecture

```{admonition} Status: Next
:class: note

`petgraph` is in `Cargo.toml` but **not currently used**. This phase will activate it.
```

### Proposed Structures

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

The `Vault` struct will change from:

```rust
pub struct Vault {
    pub md_files: MyHashMap<MDFile>,
    // ...
}
```

to:

```rust
pub struct Vault {
    pub graph: MystWorkspaceGraph,
    pub file_map: HashMap<PathBuf, NodeIndex>,
    // ...
}
```

## Phase 3: Feature Upgrades

### Completions (`src/completion/`)

Directives
: New completer for ````{name}` syntax

Roles
: New completer for `{role}` syntax

References
: `LinkCompleter` will query the graph for matching targets/anchors

### Go to Definition (`src/gotodef.rs`)

**Logic Change**: Instead of `select_referenceables_for_reference` (linear scan), query graph neighbors:

- **Input**: File URI + Cursor Position → Edge
- **Output**: Target Node URI

### Diagnostics (`src/diagnostics.rs`)

Cycle Detection
: On `didSave`, run `petgraph::algo::is_cyclic_directed` on inclusion subgraph

Orphan Check
: Find nodes with 0 in-degree (excluding root)

## Migration Checklist

- [x] Prototype Parser: Test `markdown-rs` parsing of MyST directives
- [x] TDD Integration: Tests for vault extraction
- [ ] Graph Prototype: Implement `MystWorkspaceGraph` struct
- [ ] Vault Refactor: Replace HashMap storage with Graph
- [ ] LSP Adapters: Rewrite `get_completions` and `goto_definition` to use Graph API

```{seealso}
{doc}`future-enhancements` for deferred refactorings and future MyST reference types.
```
