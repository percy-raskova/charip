---
title: Reference
---

# Reference

Complete reference documentation for charip-lsp.

## In This Section

{doc}`capabilities`
: Full list of LSP features and what they do.

{doc}`myst-syntax`
: All MyST syntax that charip-lsp understands.

{doc}`configuration`
: Configuration options and settings files.

{doc}`cli`
: Command-line interface reference.

## Rust API Documentation

For developers extending or embedding charip-lsp, the Rust API documentation provides detailed information about internal types and functions.

**[View Rust API Docs →](https://percy-raskova.github.io/charip/charip/)**

Key modules:
- **vault** - Core data structures (Vault, DocumentNode, Reference)
- **completion** - Autocomplete providers
- **myst_parser** - MyST syntax extraction
- **config** - Configuration management

## Quick Reference

### Trigger Characters

| Character | Context | Triggers |
|-----------|---------|----------|
| `[` | Text | Markdown link completion |
| `{` | Text | Role name suggestions |
| `` ` `` | After role | Role target completion |
| `(` | After `]` | Link path completion |
| `#` | Text | Tag completion |
| `>` | Line start | Callout completion |

### LSP Methods Implemented

| Method | Feature |
|--------|---------|
| `textDocument/completion` | Autocomplete |
| `textDocument/definition` | Go to definition |
| `textDocument/references` | Find references |
| `textDocument/hover` | Hover information |
| `textDocument/rename` | Rename symbol |
| `textDocument/documentSymbol` | Document outline |
| `workspace/symbol` | Workspace search |
| `textDocument/publishDiagnostics` | Error reporting |
| `textDocument/codeAction` | Quick fixes |
