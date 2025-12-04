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
