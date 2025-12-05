---
title: MyST Syntax Support
---

# MyST Syntax Support

This reference documents all [MyST syntax](inv:myst#index) that charip-lsp understands and provides intelligence for.

## Directives

```{seealso}
[MyST Roles and Directives](inv:myst#syntax/roles-and-directives) - Official documentation
```

### Detection

Directives are detected in fenced code blocks:

```markdown
```{directive-name}
:option: value

Content
`` `
```

Both backtick (` ``` `) and colon (`:::`) fences are supported.

### Parsed Directives

| Directive | Extracted Data |
|-----------|----------------|
| All directives | Name, options, content range |
| `figure`, `image` | `:name:` label, path |
| `math` | `:label:` for equation references |
| `glossary` | Term definitions |
| `toctree` | Entry list, `:caption:` |
| `include` | Target path |
| `code-block` | Language, `:caption:` |

### Directive Labels

The `:name:` option makes any directive referenceable:

```{code-block} markdown
```{note}
:name: my-note

Important information.
`` `
```

Reference with `` {ref}`my-note` ``.

## Roles

```{seealso}
[MyST Roles and Directives](inv:myst#syntax/roles-and-directives) - Official documentation
```

### Detection

Roles are detected in inline text:

```markdown
{role}`target`
{role}`display text <target>`
```

### Supported Roles

| Role | Purpose | Target Type |
|------|---------|-------------|
| `{ref}` | Cross-reference | Anchor or heading |
| `{numref}` | Numbered reference | Labeled figure/table |
| `{doc}` | Document link | File path |
| `{term}` | Glossary reference | Term name |
| `{eq}` | Equation reference | Math label |
| `{download}` | Downloadable file | File path |

### Role Parsing

For each role, charip-lsp extracts:
- Role name (ref, doc, term, etc.)
- Target text
- Display text (if different)
- Position in document

## Anchors

```{seealso}
[MyST Cross-references](inv:myst#syntax/cross-referencing) - Official documentation
```

### Syntax

```markdown
(anchor-name)=
# Heading
```

The anchor must be on the line immediately before a heading or content block.

### Heading Slugs

Every heading implicitly creates an anchor from its slug:

```markdown
# My Heading Title
```

Creates implicit anchor `my-heading-title`.

### What's Extracted

- Anchor name
- Position (line, column)
- Associated heading (if any)

## Frontmatter

```{seealso}
[MyST Configuration](inv:myst#configuration) - Official documentation
```

### Substitutions

```yaml
---
substitutions:
  project: charip-lsp
  version: 0.1.0
---
```

Also supported under `myst:`:

```yaml
---
myst:
  substitutions:
    project: charip-lsp
---
```

### Substitution Usage

```markdown
Welcome to {{project}} version {{version}}.
```

charip-lsp validates that substitutions are defined.

## Glossary Definitions

### Syntax

```{code-block} markdown
```{glossary}
Term One
  Definition of term one.

Term Two
  Definition of term two.
  Can span multiple lines.
`` `
```

### What's Extracted

- Term names
- Definitions
- Position for go-to-definition

## Cross-Reference Resolution

```{seealso}
[MyST Cross-references](inv:myst#syntax/cross-referencing) - Official documentation
```

### Forward Resolution (Go-to-Definition)

| Reference | Resolves To |
|-----------|-------------|
| `` {ref}`target` `` | `(target)=` anchor |
| `` {ref}`heading-slug` `` | Matching heading |
| `` {doc}`/path` `` | File at path |
| `` {term}`word` `` | Glossary entry |
| `` {eq}`label` `` | `{math}` with `:label:` |
| `[text](file.md)` | Linked file |
| `[text](file.md#anchor)` | Anchor in file |

### Backward Resolution (Find References)

From any anchor, heading, or file, find all references pointing to it.

## Extensions Supported

```{seealso}
[MyST Optional Syntax Extensions](inv:myst#syntax/optional) - Official documentation
```

charip-lsp supports MyST documents using these extensions:

| Extension | Syntax | Support |
|-----------|--------|---------|
| `colon_fence` | `:::directive` | Full |
| `deflist` | `term\n: definition` | Parsed (no special features) |
| `dollarmath` | `$math$`, `$$math$$` | Parsed (no special features) |
| `substitution` | `{{variable}}` | Full (validation, completion) |
| `tasklist` | `- [ ] item` | Parsed (no special features) |
| `attrs_inline` | `{#id .class}` | Parsed (no special features) |

## Limitations

### Not Parsed

- Inline code contents (`` `code` ``)
- Comment contents (`% comment`)
- Raw blocks (`{raw}`)

### Not Validated

- External URLs (http/https links)
- Sphinx domain roles (`{py:func}`, etc.)
- Custom roles defined in `conf.py`

### Known Gaps

- `:doc:` role (with colons) not yet supported (use `{doc}`)
- Nested directive content is parsed but not recursively indexed
