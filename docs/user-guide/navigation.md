---
title: Navigation
---

# Navigation

charip-lsp provides powerful navigation features to move quickly through your documentation.

## Go to Definition

Jump from a reference to its target.

### Default Keybindings

| Editor | Keybinding |
|--------|------------|
| Neovim | `gd` or `Ctrl+]` |
| VS Code | `F12` or `Ctrl+Click` |

### Supported References

| Reference | Target |
|-----------|--------|
| `` {ref}`target` `` | `(target)=` anchor or heading |
| `` {doc}`path` `` | The referenced document |
| `` {term}`word` `` | Glossary definition |
| `` {eq}`label` `` | Labeled math block |
| `[text](file.md)` | The linked file |
| `[text](file.md#heading)` | Specific heading in file |
| `#tag` | First occurrence of the tag |

### Example

```markdown
See the {ref}`installation-guide` for details.
              ↑
              Cursor here, press gd
              ↓
              Jumps to (installation-guide)= anchor
```

## Find All References

See everywhere a target is used (backlinks).

### Default Keybindings

| Editor | Keybinding |
|--------|------------|
| Neovim | `gr` |
| VS Code | `Shift+F12` |

### How It Works

Place your cursor on:
- An anchor definition `(my-anchor)=`
- A heading
- A glossary term definition
- A file path

Then trigger "find references" to see all locations that link to it.

### Use Cases

- **Impact analysis**: Before renaming, see what would be affected
- **Orphan hunting**: Find targets with zero references
- **Understanding usage**: See how a concept is used across documentation

## Document Symbols

View the structure of the current document.

### Default Keybindings

| Editor | Keybinding |
|--------|------------|
| Neovim | `:Telescope lsp_document_symbols` |
| VS Code | `Ctrl+Shift+O` |

### Symbol Types

- Headings (all levels)
- Anchors (`(target)=`)
- Directive labels (`:name:` options)

## Workspace Symbols

Search for symbols across the entire vault.

### Default Keybindings

| Editor | Keybinding |
|--------|------------|
| Neovim | `:Telescope lsp_workspace_symbols` |
| VS Code | `Ctrl+T` |

### Search Tips

- Type partial matches: `install` finds `installation-guide`
- Search is fuzzy: `instgd` might find `installation-guide`
- Results are ranked by relevance

## Hover Information

Get information about a reference without navigating away.

### Default Keybindings

| Editor | Keybinding |
|--------|------------|
| Neovim | `K` |
| VS Code | Hover with mouse |

### What You See

Hovering over a reference shows:
- The target type (anchor, heading, file, etc.)
- For files: the first few lines of content
- For headings: the heading text and file path
- For terms: the glossary definition

## Navigation Workflow

### Exploring a Codebase

1. Open the main index file
2. Use document symbols to see the structure
3. Go-to-definition on toctree entries to dive deeper
4. Use find-references to understand connections

### Following a Reference Chain

1. Go-to-definition on a reference
2. At the target, find-references to see related content
3. Navigate to other usages
4. Use editor's "go back" (`Ctrl+O` in Neovim) to return

### Finding Where Something Is Used

1. Navigate to the definition (anchor, heading, file)
2. Find-references
3. Review each usage in the references panel
