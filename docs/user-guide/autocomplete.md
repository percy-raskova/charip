---
title: Using Autocomplete
---

# Using Autocomplete

charip-lsp provides context-aware autocomplete throughout your [MyST](inv:myst#index) documents.

## Directive Completion

```{seealso}
[MyST Directive Syntax](inv:myst#syntax/roles-and-directives) for complete directive documentation.
```

When you type the opening of a fenced code block with a brace, directive suggestions appear:

```markdown
```{no
     ↓
     note
     numref
     ...
```

### Trigger

Type ` ```{ ` at the start of a line (three backticks followed by an opening brace).

### Available Directives

| Category | Directives |
|----------|------------|
| Admonitions | `note`, `warning`, `tip`, `important`, `caution`, `danger`, `hint`, `attention`, `seealso` |
| Content | `figure`, `image`, `code-block`, `literalinclude` |
| Structure | `toctree`, `include`, `contents` |
| References | `glossary` |
| Math | `math` |

## Role Name Completion

When you type an opening brace, role names are suggested:

```markdown
See the {
        ↓
        ref
        doc
        term
        numref
        eq
        download
```

### Trigger

Type `{` in running text (not after ` ``` ` or `:::` which indicate directives).

## Role Target Completion

```{seealso}
[MyST Cross-referencing](inv:myst#syntax/cross-referencing) for complete role documentation.
```

After typing a role and opening backtick, targets are suggested:

```markdown
See {ref}`inst
           ↓
           installation-guide
           installing-dependencies
           instance-configuration
```

### By Role Type

`{ref}` and `{numref}`
: Suggests all anchors (`(target)=`) and heading slugs in the vault.

`{doc}`
: Suggests all `.md` files. Paths are relative to the current file.

`{term}`
: Suggests terms defined in `{glossary}` directives.

`{eq}`
: Suggests labeled math blocks (`:label:` option on `{math}` directive).

`{download}`
: Suggests files in your project for download links.

## Root-Relative Paths

For `{doc}` and `{download}` roles, prefix with `/` to get paths from the vault root:

```markdown
{doc}`/getting-started/installation`
```

This is useful when you're deep in a subdirectory and want to reference a file by its absolute path within the documentation.

## Markdown Link Completion

Standard markdown links also get path completion:

```markdown
[installation guide](getti
                          ↓
                          getting-started/installation.md
                          getting-help.md
```

### Trigger

Type `](` after link text to trigger file path completion.

## Tag Completion

Hashtag-style tags are completed from existing tags in the vault:

```markdown
#docu
     ↓
     documentation
     docker
```

### Trigger

Type `#` followed by characters.

## Filtering

As you type more characters, the suggestions narrow:

```markdown
{ref}`install  →  Shows only anchors containing "install"
```

The fuzzy matcher handles typos and partial matches.

## Completion Behavior

### Snippet Insertion

Selecting a directive inserts a complete snippet:

```markdown
```{note}
$0
`` `
```

The cursor is positioned inside for immediate typing.

### Path Handling

- Paths to files include the `.md` extension by default
- Configure `include_md_extension_md_link` in settings to change this

## Troubleshooting

### No Completions Appear

1. Ensure the file is in a directory with `conf.py` or `.git`
2. Check that the LSP is running (`:LspInfo` in Neovim)
3. Verify the file type is recognized as markdown

### Wrong Completions

The trigger context matters:
- `{` in text → role names
- ` ```{ ` → directives
- `{role}\`` → role targets

Make sure you're in the right context for the completion you want.
