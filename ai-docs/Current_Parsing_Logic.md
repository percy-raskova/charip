# Current Parsing Logic (Legacy)

This document details the **Regex-based** parsing currently used in `src/vault/mod.rs`. This logic needs to be ported to the new AST-based parser.

## 1. Wiki Links (`[[...]]`)
*   **Regex**: `(?<filepath>[^[\]|\.#]+)?(\.(?<filepath>[^[\]|\.#]+))?#(?<infileref>[^[\]\. |]+)?(\|(?<display>[^[\]\. |]+))?\]\]`
*   **Captures**:
    *   `filepath`: The target file path.
    *   `infileref`: The anchor (heading or block ID).
    *   `ending`: File extension (e.g., `.md`).
    *   `display`: Alias text (after `|`).
*   **Logic**: `Reference::new` iterates over matches. Filters out links with non-`.md` extensions.

## 2. Markdown Links (`[...](...)`)
*   **Regex**: `\[(?<display>[^[\]\.]*?)\]\(<?(?<filepath>(\./)?[^[\]|\.#<>]+?)?(?<ending>\.[^# <>]+?)?(\#(?<infileref>[^[\]\. |<>]+?))?>?\)`
*   **Captures**: Same as Wiki Links.
*   **Logic**: Handles standard Markdown links. Supports angle brackets `<...>`. 

## 3. Headings (`# ...`)
*   **Regex**: `(?<starter>#+) (?<heading_text>.+)`
*   **Logic**: `MDHeading::new`.
    *   `level`: Length of `starter` group (e.g., `##` = 2).
    *   `heading_text`: The content.

## 4. Tags (`#...`)
*   **Regex**: `(?<full>#(?<tag>[\p{L}_/'"‘’“”-][\p{L}0-9_/'"‘’“”-]*))`
*   **Logic**: `MDTag::new`. Requires preceding whitespace/boundary. Captures unicode letters.

## 5. Block IDs (`^...`)
*   **Regex**: `.+ ( \^(?<index>\w+))`
*   **Logic**: `MDIndexedBlock::new`. Finds ` ^id` at end of lines.

## 6. Footnotes (`[^...]`)
*   **Regex**: `\[(?<index>\^[^ \[ \]]+)\]:(?<text>.+)`
*   **Logic**: `MDFootnote::new`. Parses footnote definitions.

## 7. Metadata (Frontmatter)
*   **Regex**: `^---\n(?<metadata>(\n|.)*?)\n---`
*   **Logic**: `MDMetadata::new`. Extracts YAML block between `---` delimiters. Uses `serde_yaml` to parse aliases.

## 8. Code Blocks
*   **Regex (Fenced)**: `(^|\n)(?<fullblock>``` *(?<lang>[^\n]+)?\n(?<code>(\n|.)*?)\n```)
*   **Regex (Inline)**: `(?<fullblock>`[^`\n]+?`)
*   **Logic**: `MDCodeBlock::new`. Used primarily to *exclude* ranges from other parsers (e.g., don't parse tags inside code blocks).

---

**Note for Migration**: The new `markdown-rs` parser must be configured or extended to recognize these constructs, particularly the non-standard ones like WikiLinks, Tags, and Block IDs, either via GFM extensions or custom tokenizers.
