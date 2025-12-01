---
title: Future Enhancements
---

# Future Enhancements

This document captures deferred work items and future feature ideas identified during development. Items here are **not blocking** current work but should be revisited when relevant.

## Deferred Refactorings

During the MyST integration TDD cycle, the mikado-refactor agent identified 5 refactoring opportunities. Only #1 was implemented; the rest were explicitly deferred.

```{list-table}
:header-rows: 1
:widths: 5 30 15 50

* - #
  - Refactoring
  - Priority
  - Why Deferred
* - 1
  - MystSymbolKind enum
  - HIGH
  - **DONE** - Replaced `kind: String` with enum
* - 2
  - API consistency (select_references)
  - MEDIUM
  - Medium effort, no current pain point
* - 3
  - Test helper extraction
  - LOW
  - Only 1 user (YAGNI)
* - 4
  - Generic select_field abstraction
  - LOW
  - Only 2 instances ("Rule of Three" not met)
* - 5
  - Convenience filter methods
  - LOW
  - No current callers would benefit
```

### Details

#### #2 - API Consistency

`select_references()` returns `Option<Vec<...>>` while `select_myst_symbols()` returns `Vec<...>`. The Option wrapper is arguably unnecessary since an empty Vec already communicates "no results".

```{tip}
Consider unifying to `Vec<...>` when there's a reason to touch this code.
```

#### #3 - Test Helper

`create_test_vault_dir()` in `vault/mod.rs::myst_integration_tests` could be extracted to a shared test utilities module if other integration tests need it.

#### #4 - Generic Abstraction

Both `select_references` and `select_myst_symbols` follow identical match patterns. A generic `select_field<T>()` helper could reduce duplication, but adds lifetime complexity for minimal benefit with only 2 instances.

#### #5 - Convenience Methods

Methods like `select_myst_directives()` and `select_myst_anchors()` would wrap `select_myst_symbols()` with kind filtering. Add these when LSP features need filtered results frequently.

## Future MyST Reference Types

The `MystSymbolKind::Reference` variant exists as a placeholder. These MyST cross-reference syntaxes should be parsed in future work:

```{list-table}
:header-rows: 1
:widths: 30 40 20

* - Syntax
  - Description
  - Priority
* - `` {ref}`target` ``
  - Cross-reference to anchor
  - HIGH
* - `` {doc}`path/to/doc` ``
  - Document link
  - HIGH
* - `` {term}`glossary-entry` ``
  - Glossary reference
  - MEDIUM
* - `` {numref}`figure-label` ``
  - Numbered reference
  - LOW
* - `` {eq}`equation-label` ``
  - Equation reference
  - LOW
```

### Implementation Notes

Parser location
: Extend `scan_for_myst()` in `src/myst_parser.rs`

Detection strategy
: MyST roles appear as plain text to CommonMark parsers. Use regex on `Node::Text` nodes to find `` {role}`target` `` patterns.

```{important}
Phase 2 graph architecture is needed to *resolve* these references across files. Parsing them without resolution gives data we can't act on.
```

### Recommended Approach

Option A: Parameterized Reference variant:

```rust
pub enum MystSymbolKind {
    Directive,
    Anchor,
    Reference { role: String },  // e.g., role = "ref", "doc", "term"
}
```

Option B: Specific variants if behavior differs:

```rust
pub enum MystSymbolKind {
    Directive,
    Anchor,
    RefRole,      // {ref}`target`
    DocRole,      // {doc}`path`
    TermRole,     // {term}`entry`
}
```

## Phase 2 Prerequisites

Before implementing cross-file reference resolution:

1. **Graph architecture** (`petgraph`) must be in place
2. **Vault refactor** from `HashMap<PathBuf, MDFile>` to graph-based storage
3. **Node/Edge model** for documents and relationships

```{seealso}
{doc}`myst-implementation` for Phase 2 details.
```
