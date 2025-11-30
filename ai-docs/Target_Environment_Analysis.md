# Target Environment Analysis (`~/rstnotes`)

This document analyzes the specific target environment found in `~/rstnotes` to refine the MyST Language Server roadmap.

## 1. Tech Stack & Format
*   **Build System**: Sphinx with `myst-parser`.
*   **Format**: MyST Markdown (mostly) with some reStructuredText (`.rst`) files.
*   **Extensions**: Heavily customized with local Python extensions in `_extensions/` and tools in `_tools/`.

## 2. MyST Configuration (`conf.py`)
The LSP must support the syntax enabled in `conf.py`:

*   **Extensions**:
    *   `colon_fence` (`::: directive`) - **Critical** for semantic block support.
    *   `deflist` (Definition lists).
    *   `dollarmath` (`$math$`).
    *   `fieldlist` (`:field: value`).
    *   `substitution` (`{{variable}}`).
    *   `tasklist` (`[ ]`).
    *   `attrs_inline` (`{#id .class}`).
*   **Substitutions**:
    *   Global variables defined in `conf.py` (e.g., `{{assets}}` -> `https://assets.percybrain.com`).
    *   The LSP should resolve these in Hover previews to show valid links.

## 3. Custom Roles & Directives
The environment uses custom semantics that the LSP must recognize:

*   **Roles**:
    *   `{term}`, `{ref}`, `{doc}` (Standard Sphinx/MyST).
    *   `{tag}` (Custom role defined in `rst_prolog`).
*   **Directives**:
    *   `definition` (from `_extensions/definition.py`).
    *   `honeypot` (from `_extensions/honeypot.py`).
    *   `ai_content` (from `_extensions/ai_content.py`).

## 4. Frontmatter Schema
A rigid frontmatter schema is enforced by `_tools/frontmatter_normalizer`:

*   **Identity**: `zkid` (timestamp-based ID), `title`, `author`.
*   **Timestamps**: `date-created`, `date-edited`.
*   **Taxonomy**:
    *   `category`: Enum [Concepts, Methods, Systems, Infrastructure, Miscellaneous].
    *   `tags`: Hierarchical list (e.g., `domain/topic`).
*   **Workflow**: `publish` (bool), `status` (draft/stable).

**Implication for LSP**:
*   **Completions**: Offer enum values for `category` and `status`.
*   **Validation**: Warn on unknown fields or invalid categories.
*   **Code Action**: Integrate with `_tools/frontmatter_normalizer` to auto-fix/infer metadata.

## 5. Integration Opportunities

### A. External Formatter
The project has a CLI tool: `_tools/frontmatter_normalizer/cli.py`.
*   **Feature**: The LSP's `textDocument/formatting` request should be configurable to execute this Python script.

### B. Dynamic Configuration
Parsing `conf.py` (Python) in Rust is complex.
*   **Strategy**: Use a simple regex parser to extract `myst_enable_extensions` and `myst_substitutions` lists from `conf.py` to configure the LSP dynamically.

### C. Hybrid Graph
*   **Wiki Links**: Used for "speed" (`[[note]]`).
*   **MyST Roles**: Used for "precision" (`{doc}path`).
*   **The Vault**: Must index **both** and treat them as edges in the `petgraph`.

## 6. Refined Roadmap Adjustments
1.  **Parser**: Must support `colon_fence` and `substitution` syntax immediately.
2.  **Graph**: Must verify `{{assets}}` links by substituting the value from config.
3.  **LSP**: Add `textDocument/formatting` support via shell command.
