# Parser Gap Analysis & Remediation Strategy

## 1. Current Indexing Logic Analysis

The function responsible for indexing a file is **`MDFile::new`** located in `src/vault/mod.rs`.

### The Current Workflow (`src/vault/mod.rs`)
The current parsing logic operates in a flat, subtractive manner:
1.  **Identify Code Blocks**: `MDCodeBlock::new(text)` runs a regex (`^`\`...`) to find all fenced code blocks.
2.  **Identify Elements**: It then runs separate regex passes for `Reference::new`, `MDHeading::new`, `MDTag::new`, etc.
3.  **Subtraction (Masking)**: It iterates through the found elements and **discards** any that overlap with the identified code block ranges.

```rust
// src/vault/mod.rs : MDFile::new
let code_blocks = MDCodeBlock::new(text).collect_vec();
let links = Reference::new(text, file_name)
    .filter(|it| !code_blocks.iter().any(|codeblock| codeblock.includes(it))) // <--- The Problem
    .collect_vec();
```

## 2. The MyST Structural Gap

This logic presents fundamental blockers for MyST support, specifically for **Directives** and **Roles**.

### Case A: Fenced Directives (e.g., ````{note}`)
MyST allows directives to be written using standard code fences:
```markdown
```{note}
This is a note with a [[Link]].
```
```

**Current Behavior:**
1.  `MDCodeBlock::new` matches this pattern. It sees a code block with language `{note}`.
2.  `Reference::new` finds `[[Link]]`.
3.  **The Bug**: The `filter` clause in `MDFile::new` sees that `[[Link]]` is inside the code block range. It **discards** the link.
**Consequence:** Links inside semantic blocks (admonitions, tabs, etc.) are invisible to the LSP.

### Case B: Colon Directives (e.g., `:::{note}`)
MyST also supports a colon-based syntax:
```markdown
:::{note}
This is a note with a [[Link]].
:::
```

**Current Behavior:**
1.  `MDCodeBlock::new` (regex-based) expects backticks. It **does not match** this block.
2.  `Reference::new` finds `[[Link]]`.
3.  The filter does not trigger. The link is preserved.
**Consequence:** While the link is found, the **structure is lost**. The LSP sees `[[Link]]` floating in generic text. It does not know it belongs to a `note`. It cannot provide features like "fold this note," "validate note arguments," or "highlight note title."

### Case C: Inline Roles (e.g., `{ref}target`)
**Current Behavior:**
1.  `Reference::new` uses `WIKI_LINK_RE` (`[[...]]`) and `MD_LINK_RE` (`[...](...)`).
2.  There is no regex for `{role}`.
**Consequence:** Syntax like `{ref}my-target` is treated as plain text. No go-to-definition, no validation.

## 3. Proposed Remediation (The `myst_parser` Integration)

To fix this, we must move from **Regex Subtraction** to **AST Traversal**.

### The Plan
The new `src/myst_parser.rs` (already partially implemented) uses `markdown-rs` to generate an AST. This allows us to distinguish between "structural" code blocks (MyST directives) and "literal" code blocks (Python/Rust code).

### New Indexing Logic (`MDFile::new` Refactor)

Instead of independent regex passes, `MDFile::new` should:

1.  **Parse AST**: Call `myst_parser::parse(text)`.
2.  **Visit Nodes**: Traverse the tree.
    *   **If `Node::Code`**:
        *   Check the info string (e.g., `{note}`).
        *   **If Directive**: Recurse into the body (parse content as Markdown). Record the container (e.g., `MystNode::Admonition`).
        *   **If Code**: Treat as opaque (preserve current behavior).
    *   **If `Node::Text`**: Run a lightweight regex *only on text nodes* to find inline Roles (`{ref}`).
3.  **Construct Graph**: Populate `MDFile` structs from this semantic tree.

### Immediate Action Items
1.  Update `src/myst_parser.rs` to handle recursion for specific directives (currently it flattens the body to a string).
2.  Modify `MDFile::new` to use `myst_parser` results instead of `MDCodeBlock` regexes for exclusion logic.
3.  Implement a `Role` matcher in `src/myst_parser.rs`.

```