---
title: LSP Capabilities
---

# LSP Capabilities

This document provides a comprehensive overview of what charip-lsp can do for [MyST](inv:myst#index) documentation projects.

## At a Glance

| Category | Features |
|----------|----------|
| **Navigation** | Go-to-definition, find references, backlinks |
| **Completions** | Directives, roles, anchors, documents, glossary terms, equations |
| **Diagnostics** | Broken links, missing images, undefined substitutions, include cycles |
| **Refactoring** | Rename anchors, headings, files with reference updates |
| **Analysis** | Orphan detection, transitive dependencies, toctree hierarchy |

---

## Completions

```{seealso}
[MyST Roles and Directives](inv:myst#syntax/roles-and-directives) - Official syntax documentation
```

charip-lsp provides intelligent autocomplete for [MyST syntax](inv:myst#index).

### Directive Completion

Typing ` ```{ ` triggers directive name suggestions:

```markdown
```{no⏎  →  Suggests: note, numref, ...
```

Supported directive types:
- Admonitions: `note`, `warning`, `tip`, `important`, `caution`, `danger`, `error`, `hint`, `attention`, `seealso`
- Content: `figure`, `image`, `code-block`, `literalinclude`
- Structure: `toctree`, `include`, `contents`
- Math: `math`
- Glossary: `glossary`
- Custom: Any directive registered in your Sphinx conf.py

### Role Target Completion

Typing a role prefix triggers target suggestions:

| Role | Trigger | Completes From |
|------|---------|----------------|
| `{ref}` | `` {ref}` `` | All anchors (`(target)=`) and heading slugs |
| `{numref}` | `` {numref}` `` | Labeled figures and tables |
| `{doc}` | `` {doc}` `` | All `.md` files in vault |
| `{term}` | `` {term}` `` | Glossary definitions |
| `{eq}` | `` {eq}` `` | Labeled math blocks |
| `{download}` | `` {download}` `` | Asset files |

Example:
```markdown
See {ref}`inst⏎  →  Suggests: installation-guide, installing-deps, ...
```

### Markdown Link Completion

Standard markdown links get path completion:

```markdown
[click here](gett⏎  →  Suggests: getting-started.md, getting-help.md, ...
```

### Tag Completion

Hashtag-style tags are autocompleted from existing tags in the vault:

```markdown
#doc⏎  →  Suggests: #documentation, #docker, ...
```

---

## Navigation

### Go to Definition

**Ctrl+Click** or **F12** on any reference jumps to its target:

| Reference Type | Target |
|----------------|--------|
| `{ref}`target`` | `(target)=` anchor or heading |
| `{doc}`path`` | The referenced document |
| `{term}`word`` | Glossary definition |
| `{eq}`label`` | Labeled math block |
| `[text](file.md)` | The linked file |
| `[text](file.md#heading)` | Specific heading in file |
| `#tag` | Tag definition (first occurrence) |

### Find All References (Backlinks)

**Shift+F12** on any target shows all documents that reference it:

- Find all uses of an anchor
- Find all links to a document
- Find all uses of a glossary term
- Find all occurrences of a tag

Performance: O(K) where K = number of incoming references, thanks to graph-based traversal.

---

## Diagnostics

charip-lsp reports problems in real-time as you type.

### Broken References

```markdown
See {ref}`nonexistent-anchor`  ⚠️ Unknown anchor 'nonexistent-anchor'
```

Detected for:
- `{ref}` roles pointing to undefined anchors
- `{doc}` roles pointing to non-existent files
- `{term}` roles with undefined glossary terms
- `{eq}` roles with missing equation labels
- Markdown links to missing files

### Missing Images

```markdown
![diagram](images/missing.png)  ⚠️ Missing image file 'images/missing.png'
```

Local image paths are validated. External URLs (http/https) are skipped.

### Undefined Substitutions

```markdown
The {{undefined_var}} value  ⚠️ Undefined substitution '{{undefined_var}}'
```

Substitutions must be defined in frontmatter:
```yaml
---
substitutions:
  project_name: charip-lsp
---
```

### Include Cycle Detection

Circular includes are detected and reported:

````text
# a.md
```{include} b.md
```

# b.md
```{include} a.md
```
⚠️ Include cycle detected: a.md → b.md → a.md
````

Uses Tarjan's strongly connected components algorithm for efficient detection.

---

## Refactoring

### Rename Symbol

**F2** on a renameable symbol updates all references:

| Symbol Type | What Gets Updated |
|-------------|-------------------|
| Anchor `(target)=` | All `{ref}` and `{numref}` roles referencing it |
| Heading `# Title` | The heading text and all `[](file#heading)` links |
| File | The filename and all `{doc}` roles and markdown links |
| Tag `#topic` | All occurrences of the tag |

Example: Renaming anchor `(install)=` to `(installation)=` updates:
- The anchor definition itself
- All `` {ref}`install` `` → `` {ref}`installation` ``
- All `` {numref}`install` `` → `` {numref}`installation` ``

---

## Vault Analysis

These features leverage the petgraph-based document graph.

### Orphan Document Detection

Find documents not included in any toctree:

```rust
// API (available for editor extensions)
vault.find_orphan_documents(&root_index_path)
```

Useful for:
- Ensuring all content is discoverable
- Finding abandoned drafts
- Validating documentation completeness

### Transitive Dependencies

Find all documents a file depends on (directly or indirectly):

```rust
vault.transitive_dependencies(&path)
// Returns: HashSet of all linked/included files
```

### Transitive Dependents (Impact Analysis)

Find all documents that would be affected by changing a file:

```rust
vault.transitive_dependents(&path)
// Returns: HashSet of all files linking to this one
```

### Toctree Hierarchy

The document graph tracks `{toctree}` relationships:

```
index.md
├── getting-started.md
├── user-guide/
│   ├── index.md
│   ├── configuration.md
│   └── advanced.md
└── api-reference.md
```

This enables:
- Breadcrumb navigation hints
- Table of contents validation
- Structural analysis

---

## Supported MyST Syntax

```{seealso}
For detailed syntax documentation, see:
- [Roles and Directives](inv:myst#syntax/roles-and-directives)
- [Cross-references](inv:myst#syntax/cross-referencing)
- [Optional Extensions](inv:myst#syntax/optional)
```

### Directives Parsed

| Directive | Extracted Data |
|-----------|----------------|
| Admonitions | Name, title, content |
| `{figure}` | Path, `:name:` label, caption |
| `{math}` | `:label:` for equation references |
| `{glossary}` | Term definitions |
| `{toctree}` | Entry list, caption |
| `{include}` | Target path |

### Roles Parsed

| Role | Purpose |
|------|---------|
| `{ref}` | Cross-reference to anchor |
| `{numref}` | Numbered reference to figure/table |
| `{doc}` | Document reference |
| `{term}` | Glossary term |
| `{eq}` | Equation reference |
| `{download}` | Downloadable file |
| `{abbr}` | Abbreviation with tooltip |

### Anchors Recognized

```markdown
(my-anchor)=
# Heading After Anchor
```

The `(target)=` syntax creates a linkable anchor.

### Frontmatter Fields

```yaml
---
substitutions:
  project: charip-lsp
  version: 0.1.0
myst:
  substitutions:
    author: Documentation Team
---
```

Both `substitutions` and `myst.substitutions` are merged and validated.

---

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|------------|-------|
| Forward resolution (go-to-def) | O(K) | K = edges from source |
| Backlink resolution | O(K) | K = incoming edges |
| Path lookup | O(1) | HashMap index |
| Cycle detection | O(V+E) | Tarjan SCC |
| Orphan detection | O(V+E) | BFS traversal |
| Transitive queries | O(V+E) | DFS traversal |

The graph-based architecture replaced O(N) linear scans with efficient graph traversals.

---

## Test Coverage

charip-lsp has comprehensive test coverage:

| Component | Test Count |
|-----------|------------|
| Vault/Graph | 180+ |
| Completions | 40+ |
| Diagnostics | 60+ |
| References | 30+ |
| Rename | 20+ |
| **Total** | **347** |

All features are tested against the `TestFiles/` sample vault.
