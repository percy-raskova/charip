---
title: Feature Implementation
---

# Feature Implementation

This document maps LSP features to their implementation details in `src/`.

## Completions

**Entry**: `src/completion/mod.rs` → `get_completions`

The system constructs a chain of "Completers". If one matches the cursor context, it generates completions.

### Context Detection

Each completer has a `construct` method that checks the current line/character:

| Completer | Trigger |
|-----------|---------|
| `UnindexedBlockCompleter` | Block link syntax |
| `MarkdownLinkCompleter` | `[display](path...)` |
| `WikiLinkCompleter` | `[[` |
| `TagCompleter` | Tag patterns |
| `FootnoteCompleter` | `[^` |
| `CalloutCompleter` | `> [!` |

### Generation Process

1. Queries `Vault::select_referenceable_nodes(None)` to get all possible targets
2. Uses `nucleo-matcher` for fuzzy ranking candidates
3. Generates LSP `TextEdit` to insert the completion

```{note}
`UnindexedBlockCompleter` can generate `nanoid` block IDs (`^...`) if they don't exist.
```

## References / Backlinks

**Entry**: `src/references.rs` → `references`

1. Identifies the object under the cursor:
   - Is it a `Reference`? → Find target `Referenceable`, then find all other references
   - Is it a `Referenceable`? → Find all references to it
2. `Vault::select_references_for_referenceable` performs a parallel scan (`rayon`) to find matches

## Go to Definition

**Entry**: `src/gotodef.rs` → `goto_definition`

1. Finds the `Reference` under the cursor
2. Resolves it to `Referenceable` targets using `Vault::select_referenceables_for_reference`
3. Returns the file path and range of the target

## Rename

**Entry**: `src/rename.rs` → `rename`

1. Identifies the `Referenceable` being renamed
2. **Update Target**: Creates `WorkspaceEdit` to rename file or update heading text
3. **Update References**: Finds all backlinks via `Vault::select_references_for_referenceable`
4. **Iterate**: For each reference, calculates new link text and adds `TextEdit`

```{warning}
This logic manually reconstructs links, which can be complex for mixed Wiki/Markdown link styles.
```

## Diagnostics

**Entry**: `src/diagnostics.rs` → `diagnostics`

Trigger
: Runs on `did_open`, `did_save`

Analysis
: `path_unresolved_references` finds all references not pointing to a known `Referenceable`

Output
: Returns `Diagnostic` objects for every broken link

## Hover

**Entry**: `src/hover.rs` → `hover`

1. Identifies the item under the cursor
2. `ui::preview_referenceable` / `ui::preview_reference` generates content
3. Combines:
   - Content of the referenced note/block (first few lines)
   - List of "Backlinks" (first 20 references)

## Code Actions

**Entry**: `src/codeactions.rs` → `code_actions`

Create Unresolved File
: Detects unresolved `Reference`s and offers to create the file

Append Heading
: Offers to append a missing heading to an existing file

## Daily Notes

**Logic**: `src/daily.rs` & `src/commands.rs`

- Parses relative dates (`today`, `next friday`) using `fuzzydate` and `chrono`
- Checks file names against the configured `dailynote` format string
