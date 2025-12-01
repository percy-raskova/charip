---
title: Parser Gap Analysis
---

# Parser Gap Analysis & Remediation Strategy

## Current Indexing Logic

The function responsible for indexing a file is **`MDFile::new`** located in `src/vault/mod.rs`.

### The Current Workflow

The current parsing logic operates in a flat, subtractive manner:

1. **Identify Code Blocks**: `MDCodeBlock::new(text)` runs a regex to find all fenced code blocks
2. **Identify Elements**: Separate regex passes for `Reference::new`, `MDHeading::new`, `MDTag::new`, etc.
3. **Subtraction (Masking)**: Discard any elements that overlap with code block ranges

```rust
// src/vault/mod.rs : MDFile::new
let code_blocks = MDCodeBlock::new(text).collect_vec();
let links = Reference::new(text, file_name)
    .filter(|it| !code_blocks.iter().any(|codeblock| codeblock.includes(it)))
    .collect_vec();
```

## The MyST Structural Gap

This logic presents fundamental blockers for MyST support.

### Case A: Fenced Directives

MyST allows directives using standard code fences:

````markdown
```{note}
This is a note with a [[Link]].
```
````

```{warning}
**Current Behavior Bug**: The filter sees `[[Link]]` inside a code block range and **discards** it.

**Consequence**: Links inside semantic blocks are invisible to the LSP.
```

### Case B: Colon Directives

MyST also supports colon-based syntax:

```markdown
:::{note}
This is a note with a [[Link]].
:::
```

**Current Behavior**:
1. `MDCodeBlock::new` expects backticks—does not match
2. `Reference::new` finds `[[Link]]`
3. Link is preserved, but **structure is lost**

The LSP sees `[[Link]]` as floating text. It cannot:
- Fold the note
- Validate note arguments
- Highlight note title

### Case C: Inline Roles

**Current Behavior**:
- `Reference::new` uses `WIKI_LINK_RE` and `MD_LINK_RE`
- No regex for `{role}`

**Consequence**: `{ref}my-target` is treated as plain text.

## Proposed Remediation

To fix this, we move from **Regex Subtraction** to **AST Traversal**.

### New Indexing Logic

Instead of independent regex passes, `MDFile::new` should:

```{list-table}
:header-rows: 1
:widths: 10 40 50

* - Step
  - Action
  - Details
* - 1
  - Parse AST
  - Call `myst_parser::parse(text)`
* - 2
  - Visit Nodes
  - Traverse tree, handling each node type
* - 3
  - Construct Graph
  - Populate `MDFile` from semantic tree
```

### Node Handling Logic

For `Node::Code`:
: Check info string. If directive (e.g., `{note}`), recurse into body. If code, treat as opaque.

For `Node::Text`:
: Run lightweight regex *only on text nodes* to find inline Roles.

### Immediate Action Items

- [ ] Update `src/myst_parser.rs` to handle recursion for specific directives
- [ ] Modify `MDFile::new` to use `myst_parser` results for exclusion logic
- [ ] Implement a `Role` matcher in `src/myst_parser.rs`

```{seealso}
{doc}`myst-implementation` for the overall roadmap.
```
