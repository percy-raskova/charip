# MyST LSP Roadmap for Percypedia

**Created**: 2025-12-01
**Based on**: Analysis of ~/rstnotes (34 content files, 98 files with frontmatter)
**Status**: Planning

---

## Context

This LSP (charip-lsp, forked from markdown-oxide) is being adapted for a specific MyST/Sphinx environment: the Percypedia knowledge base at ~/rstnotes.

### Current State (as of 2025-12-01)

| Component | Status |
|-----------|--------|
| AST-based parsing | Complete (`src/vault/ast_refs.rs`) |
| MyST directive extraction | Partial (`src/myst_parser.rs` - directives, anchors) |
| Wikilink support | Removed (MyST uses standard MD links) |
| Module structure | Extracted (`types.rs`, `helpers.rs`, `tests/`) |
| Tests passing | 77 |

---

## Target Environment Analysis

### MyST Extensions Enabled (conf.py)

```python
myst_enable_extensions = [
    "colon_fence",    # Not used in practice
    "deflist",        # Not observed
    "dollarmath",     # Minimal use
    "fieldlist",      # Not observed
    "substitution",   # Used ({assets}, honeypot vars)
    "tasklist",       # Not observed
    "attrs_inline",   # Not observed
]
```

### Actual Usage (by frequency)

| Feature | Count | Priority |
|---------|-------|----------|
| `{doc}` role | 50 | **High** |
| Admonitions (warning, important, note) | 37 | **High** |
| `{ref}` role | 10 | **High** |
| `figure` directive | 9 | Medium |
| `{term}` role | 7 | Medium |
| Frontmatter | 98 files | Medium |
| `{func}` role | 3 | Low |
| Custom directives (definition, ai-chat) | ~10 | Low |

---

## Tier 1: High-Priority Features

### 1.1 Document Link Completion (`{doc}` role)

**Current usage**: 50 instances
**Pattern**: `` {doc}`/path/to/document` ``

**LSP Capabilities**:
- Completion: List valid document paths when typing inside `` {doc}` ``
- Validation: Diagnostic for broken document links
- Go-to-definition: Jump to target document

**Implementation notes**:
- Vault already tracks all MD files
- Need to extract role syntax from AST (appears as text, not native markdown)
- Path resolution relative to content root

### 1.2 Admonition Completion

**Current usage**: 37 instances
**Directives used**: `warning` (15), `important` (12), `note` (10), `admonition` (8), `seealso` (3), `danger` (2), `tip` (1), `hint` (1)

**LSP Capabilities**:
- Completion: Suggest directive names when typing `` ``` ``
- Snippets: Insert scaffold with placeholder content

**Implementation notes**:
- `myst_parser.rs` already extracts directives from code fence `lang` field
- Need completion provider triggered in code fence context

### 1.3 Anchor Reference Completion (`{ref}` role)

**Current usage**: 10 instances
**Pattern**: `` {ref}`section-label` ``

**LSP Capabilities**:
- Completion: List named anchors (from `(target)=` syntax)
- Go-to-definition: Jump to anchor location
- Find-references: Find all references to an anchor

**Implementation notes**:
- `myst_parser.rs` already extracts anchors (`MystSymbolKind::Anchor`)
- Need to build anchor index in Vault
- Need role extraction from text nodes

---

## Tier 2: Medium-Priority Features

### 2.1 Glossary Term Completion (`{term}` role)

**Current usage**: 7 instances
**Pattern**: `` {term}`dialectical materialism` ``

**LSP Capabilities**:
- Completion: List terms from glossary
- Hover: Show term definition

**Implementation notes**:
- Glossary defined in `glossary` directive blocks
- Need to parse glossary content and build term index
- Definition directive also creates terms (format: `term-{name}`)

### 2.2 Figure Path Completion

**Current usage**: 9 instances
**Pattern**: `{assets}/images/diagram.png` in figure directives

**LSP Capabilities**:
- Completion: Path completion for image files
- Validation: Flag missing images

**Implementation notes**:
- `{assets}` is a substitution pointing to external URL
- Local assets may exist in `_static/` or similar
- Need to determine if local path completion is useful

### 2.3 Frontmatter Validation

**Current usage**: 98 files with frontmatter
**Schema**: `_schemas/frontmatter.schema.json`

**LSP Capabilities**:
- Validation: Validate frontmatter against JSON schema
- Completion: Suggest field names
- Completion: Suggest enum values (`status: draft|review|complete`)

**Implementation notes**:
- Schema already exists and is machine-readable
- `src/vault/metadata.rs` handles frontmatter parsing
- Need to integrate JSON schema validation

**Frontmatter fields** (all optional):
- `zkid`: 12-digit timestamp (YYYYMMDDHHMM)
- `title`: Document title
- `description`: Max 160 chars (SEO)
- `author`: Defaults to 'Percy'
- `date-created`, `date-edited`: YYYY-MM-DD
- `category`, `subcategory`: Navigation grouping
- `tags`: Array of hierarchical tags (`theory/marxism`)
- `publish`: Boolean (default true)
- `status`: Enum (draft, review, complete)

---

## Tier 3: Low-Priority Features

### 3.1 Function Reference Completion (`{func}` role)

**Current usage**: 3 instances
**Implementation**: Deferred (low usage)

### 3.2 Custom Directive Scaffolds

**Directives**:
- `definition` (1 use) - Term definition cards
- `ai-chat` (3 uses) - AI conversation blocks
- `ai-exchange` (3 uses) - Single Q&A blocks
- `category-nav` (4 uses) - Auto-navigation

**LSP Capabilities**:
- Snippets: Insert directive scaffold with required fields

**Implementation**: Deferred (low usage, snippet-only)

---

## Not Planned

These features are enabled in conf.py but not used in practice:

- Colon fence `:::` syntax (0 uses)
- Definition lists
- Task lists
- Inline attributes
- `{numref}`, `{eq}` roles

---

## Implementation Order

### Phase 1: Role Extraction Infrastructure

Before implementing any role completion, we need:
1. Extract MyST roles from text nodes (they appear as unparsed text)
2. Build role regex or parser (pattern: `{role}\`target\``)
3. Add role extraction to `ast_refs.rs` or new module

### Phase 2: Document Link Completion

1. Implement `{doc}` role extraction
2. Build document path index in Vault
3. Completion provider for `{doc}` context
4. Diagnostic for broken links

### Phase 3: Anchor Resolution

1. Build anchor index from `MystSymbol::Anchor`
2. Implement `{ref}` role extraction
3. Completion provider for `{ref}` context
4. Go-to-definition for anchors

### Phase 4: Directive Completion

1. Completion provider in code fence context
2. Static list of known directives
3. Optional: Parse conf.py for enabled extensions

### Phase 5: Glossary Integration

1. Parse `glossary` directive blocks for terms
2. Parse `definition` directive for terms
3. Implement `{term}` role completion
4. Hover provider for term definitions

### Phase 6: Frontmatter Support

1. Load JSON schema from `_schemas/frontmatter.schema.json`
2. Validation diagnostics for frontmatter
3. Completion for field names and enum values

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/myst_parser.rs` | Add role extraction |
| `src/vault/mod.rs` | Add anchor index, document index |
| `src/completion/` | New completion providers |
| `src/diagnostics.rs` | Add MyST-specific diagnostics |
| `src/hover.rs` | Term definition hover |
| `src/gotodef.rs` | Anchor go-to-definition |

---

## Dependencies

Current:
- `markdown` (markdown-rs) - AST parsing

May need:
- `jsonschema` or similar - Frontmatter validation
- None for basic role/directive completion

---

## Success Criteria

1. `{doc}` completion works for document paths
2. `{ref}` completion works for anchors
3. Directive names complete in code fences
4. Broken `{doc}` links produce diagnostics
5. Frontmatter validates against schema

---

## Notes

- This roadmap is based on actual usage analysis, not theoretical MyST coverage
- Features are prioritized by observed frequency in ~/rstnotes
- Custom extensions (ai_content, definition) are low priority due to low usage
- The goal is a useful LSP for this specific knowledge base, not a general MyST LSP
