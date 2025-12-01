---
title: Target Environment Analysis
---

# Target Environment Analysis

This document analyzes the specific target environment (`~/rstnotes`) to refine the MyST Language Server roadmap.

## Tech Stack & Format

Build System
: Sphinx with `myst-parser`

Format
: MyST Markdown (mostly) with some reStructuredText (`.rst`) files

Extensions
: Heavily customized with local Python extensions in `_extensions/` and tools in `_tools/`

## MyST Configuration

The LSP must support the syntax enabled in the target `conf.py`:

### Extensions

```{list-table}
:header-rows: 1
:widths: 30 70

* - Extension
  - Purpose
* - `colon_fence`
  - `::: directive` syntax—**critical** for semantic block support
* - `deflist`
  - Definition lists
* - `dollarmath`
  - `$math$` syntax
* - `fieldlist`
  - `:field: value` syntax
* - `substitution`
  - `{{variable}}` syntax
* - `tasklist`
  - `[ ]` checkboxes
* - `attrs_inline`
  - `{#id .class}` attributes
```

### Substitutions

Global variables are defined in `conf.py` (e.g., `{{assets}}` → `https://assets.percybrain.com`).

```{tip}
The LSP should resolve these in Hover previews to show valid links.
```

## Custom Roles & Directives

The environment uses custom semantics the LSP must recognize:

### Roles

| Role | Source |
|------|--------|
| `{term}`, `{ref}`, `{doc}` | Standard Sphinx/MyST |
| `{tag}` | Custom role defined in `rst_prolog` |

### Directives

| Directive | Source |
|-----------|--------|
| `definition` | `_extensions/definition.py` |
| `honeypot` | `_extensions/honeypot.py` |
| `ai_content` | `_extensions/ai_content.py` |

## Frontmatter Schema

A rigid frontmatter schema is enforced by `_tools/frontmatter_normalizer`:

```{list-table}
:header-rows: 1
:widths: 20 30 50

* - Category
  - Fields
  - Notes
* - Identity
  - `zkid`, `title`, `author`
  - `zkid` is timestamp-based ID
* - Timestamps
  - `date-created`, `date-edited`
  - Auto-managed
* - Taxonomy
  - `category`, `tags`
  - `category` is enum, `tags` are hierarchical
* - Workflow
  - `publish`, `status`
  - `status` is draft/stable
```

### LSP Implications

Completions
: Offer enum values for `category` and `status`

Validation
: Warn on unknown fields or invalid categories

Code Action
: Integrate with `_tools/frontmatter_normalizer` for auto-fix

## Integration Opportunities

### External Formatter

The project has a CLI tool: `_tools/frontmatter_normalizer/cli.py`.

```{note}
The LSP's `textDocument/formatting` request should be configurable to execute this Python script.
```

### Dynamic Configuration

Parsing `conf.py` (Python) in Rust is complex.

**Strategy**: Use simple regex parser to extract `myst_enable_extensions` and `myst_substitutions` from `conf.py`.

### Hybrid Graph

Wiki Links
: Used for "speed" (`[[note]]`)

MyST Roles
: Used for "precision" (`{doc}path`)

The Vault
: Must index **both** and treat them as edges in `petgraph`

## Roadmap Adjustments

Based on this analysis:

1. **Parser**: Must support `colon_fence` and `substitution` syntax immediately
2. **Graph**: Must verify `{{assets}}` links by substituting values from config
3. **LSP**: Add `textDocument/formatting` support via shell command
