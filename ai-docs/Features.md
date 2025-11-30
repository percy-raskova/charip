# Feature Implementation

This document maps LSP features to their implementation details in `src/`.

## Completions (`textDocument/completion`)
**Entry**: `src/completion/mod.rs` -> `get_completions`

The system attempts to construct a chain of "Completers". If one matches the cursor context, it generates completions.

1.  **Context Detection**: Each completer has a `construct` method that checks the current line/character.
    *   **`UnindexedBlockCompleter`**: Checks if the user is typing a block link.
    *   **`MarkdownLinkCompleter`**: Regex `PARTIAL_MDLINK_REGEX` checks for `[display](path...)`.
    *   **`WikiLinkCompleter`**: Scans backward for `[[`.
    *   **`TagCompleter`**: Regex `PARTIAL_TAG_REGEX`.
    *   **`FootnoteCompleter`**: Checks for `[^`.
    *   **`CalloutCompleter`**: Checks for `> [!`.

2.  **Generation**:
    *   Queries `Vault::select_referenceable_nodes(None)` to get all possible targets.
    *   **Fuzzy Matching**: Uses `nucleo-matcher` (`src/completion/matcher.rs`) to rank candidates against the typed text.
    *   **Text Edits**: Generates the LSP `TextEdit` to insert the completion. Handles nuances like inserting block IDs (`^...`) if they don't exist (`UnindexedBlockCompleter` generates a `nanoid`).

## References / Backlinks (`textDocument/references`)
**Entry**: `src/references.rs` -> `references`

1.  Identifies the object under the cursor:
    *   Is it a `Reference` (outgoing link)? -> Find the target `Referenceable`, then find all other references to it.
    *   Is it a `Referenceable` (target)? -> Find all references to it.
2.  **Search**: `Vault::select_references_for_referenceable` performs a parallel scan (`rayon`) of all vault references to find matches.

## Go to Definition (`textDocument/definition`)
**Entry**: `src/gotodef.rs` -> `goto_definition`

1.  Finds the `Reference` under the cursor.
2.  Resolves it to a list of `Referenceable` targets using `Vault::select_referenceables_for_reference`.
3.  Returns the file path and range of the target.

## Rename (`textDocument/rename`)
**Entry**: `src/rename.rs` -> `rename`

1.  Identifies the `Referenceable` being renamed.
2.  **Update Target**: Creates a `WorkspaceEdit` to rename the file or update the heading text.
3.  **Update References**: Finds all backlinks (`Vault::select_references_for_referenceable`).
4.  **Iterate**: For each reference, calculates the new link text (e.g., updating `[[Old Name]]` to `[[New Name]]`) and adds a `TextEdit` to the `WorkspaceEdit`.
    *   *Note*: This logic manually reconstructs links, which can be complex for mixed Wiki/Markdown link styles.

## Diagnostics (`textDocument/publishDiagnostics`)
**Entry**: `src/diagnostics.rs` -> `diagnostics`

1.  **Trigger**: Runs on `did_open`, `did_save` (implied by `update_vault`).
2.  **Analysis**: `path_unresolved_references` finds all references in the file that do not point to a known `Referenceable`.
3.  **Output**: Returns `Diagnostic` objects for every broken link.

## Hover (`textDocument/hover`)
**Entry**: `src/hover.rs` -> `hover`

1.  Identifies the item under the cursor.
2.  **Content Generation**: `ui::preview_referenceable` / `ui::preview_reference`.
3.  **Format**: Combines:
    *   The content of the referenced note/block (first few lines).
    *   A list of "Backlinks" (first 20 references to this item).

## Code Actions (`textDocument/codeAction`)
**Entry**: `src/codeactions.rs` -> `code_actions`

*   **Create Unresolved File**: Detects unresolved `Reference`s in the selection range. Offers to create the file at the configured path (handling Daily Note logic if applicable).
*   **Append Heading**: Offers to append a missing heading to an existing file.

## Daily Notes
**Logic**: `src/daily.rs` & `src/commands.rs`
*   Logic for parsing relative dates (`today`, `next friday`) uses `fuzzydate` and `chrono`.
*   Checks file names against the configured `dailynote` format string.
