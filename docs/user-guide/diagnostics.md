---
title: Understanding Diagnostics
---

# Understanding Diagnostics

charip-lsp reports problems in real-time as you edit. This guide explains each diagnostic and how to resolve it.

## Broken References

### Unknown Anchor

```markdown
See {ref}`nonexistent-anchor`
         ~~~~~~~~~~~~~~~~~~~~
         ⚠️ Unknown anchor 'nonexistent-anchor'
```

**Cause**: The anchor `(nonexistent-anchor)=` doesn't exist anywhere in the vault.

**Resolution**:
1. Check for typos in the anchor name
2. Create the anchor if it should exist
3. Use autocomplete to find the correct anchor name

### Missing Document

```markdown
See {doc}`/path/to/missing`
          ~~~~~~~~~~~~~~~~
          ⚠️ Document not found: '/path/to/missing'
```

**Cause**: The referenced file doesn't exist.

**Resolution**:
1. Check the path is correct
2. Create the document if needed
3. Use autocomplete to find existing documents

### Undefined Term

```markdown
The {term}`undefined-term` concept
           ~~~~~~~~~~~~~~
           ⚠️ Undefined glossary term 'undefined-term'
```

**Cause**: The term isn't defined in any `{glossary}` directive.

**Resolution**:
1. Add the term to your glossary
2. Check for spelling differences
3. Use autocomplete to reference existing terms

### Missing Equation Label

```markdown
See Equation {eq}`missing-label`
                  ~~~~~~~~~~~~~
                  ⚠️ Unknown equation label 'missing-label'
```

**Cause**: No `{math}` directive has `:label: missing-label`.

**Resolution**:
1. Add the label to the math block
2. Check the label spelling

## Missing Images

```markdown
![Architecture](images/diagram.png)
                ~~~~~~~~~~~~~~~~~~
                ⚠️ Image file not found: 'images/diagram.png'
```

**Cause**: The image file doesn't exist at the specified path.

**Resolution**:
1. Check the path is correct (relative to current file)
2. Add the missing image
3. Update the path to the correct location

**Note**: External URLs (http/https) are not validated.

## Circular Includes

```markdown
```{include} other.md
            ~~~~~~~~
            ⚠️ Include cycle detected: current.md → other.md → current.md
`` `
```

**Cause**: Document A includes B, which includes A (directly or through a chain).

**Resolution**:
1. Review the include chain
2. Restructure to break the cycle
3. Extract common content to a third file

## Undefined Substitutions

```markdown
Welcome to {{project_name}}!
           ~~~~~~~~~~~~~~~~~
           ⚠️ Undefined substitution 'project_name'
```

**Cause**: The substitution variable isn't defined.

**Resolution**:

Define in frontmatter:
```yaml
---
substitutions:
  project_name: charip-lsp
---
```

Or in `conf.py`:
```python
myst_substitutions = {
    'project_name': 'charip-lsp',
}
```

## Diagnostic Severity

| Severity | Meaning |
|----------|---------|
| Error | Prevents successful build |
| Warning | May cause problems, should be fixed |
| Information | Suggestion for improvement |
| Hint | Style or best practice suggestion |

Most charip-lsp diagnostics are warnings—your documentation will still build, but links will be broken.

## Configuring Diagnostics

### Disabling Diagnostics

In `.moxide.toml`:

```toml
[diagnostics]
unresolved_references = false
missing_images = false
```

### Per-File Suppression

Currently not supported. If you need to suppress warnings for specific files, disable the diagnostic category globally.

## Workflow Integration

### Pre-Commit Checks

Use diagnostics as part of your quality checks:

1. Open the project in your editor
2. Check the diagnostics panel for warnings
3. Resolve all issues before committing

### CI Integration

While charip-lsp is primarily an editor tool, you can use Sphinx's own link checking for CI:

```bash
sphinx-build -b linkcheck docs docs/_build/linkcheck
```

## Troubleshooting

### Diagnostics Not Appearing

1. Verify LSP is connected (`:LspInfo` in Neovim)
2. Check that the file is saved
3. Ensure the file is in a vault (has `conf.py` or `.git` parent)

### Stale Diagnostics

If diagnostics don't update after fixing:

1. Save the file
2. If still stale, close and reopen the file
3. As a last resort, restart the LSP server
