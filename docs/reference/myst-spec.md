# MyST Specification Reference

```{admonition} AI Pair Coding Reference
:class: tip

This document serves as a quick reference for AI assistants implementing MyST support in charip-lsp.
The authoritative specification is at [mystmd.org/spec](https://mystmd.org/spec).
```

## Overview

**MyST (Markedly Structured Text)** is a superset of CommonMark Markdown with extension mechanisms for:

- **Directives**: Block-level extensions (callouts, figures, code blocks, tabs)
- **Roles**: Inline extensions (references, citations, inline math)
- **Targets/Anchors**: Referenceable locations in documents
- **Cross-references**: Links between documents and targets

The MyST AST builds on:
1. **unist** (Universal Syntax Tree)
2. **mdast** (Markdown AST)
3. **MyST-specific nodes** for directives, roles, admonitions, citations, equations

## Directive Syntax

### Basic Structure

````markdown
```{directivename} arguments
:option1: value1
:option2: value2

Directive content here
```
````

### Components

| Component | Description | Required |
|-----------|-------------|----------|
| `{directivename}` | Directive identifier | Yes |
| `arguments` | Space-separated words after name | No |
| `:key: value` | Colon-style options | No |
| Body content | Content excluding options | No |

### Option Formats

**Colon-style** (recommended for few options):
````markdown
```{note}
:class: warning
Content here
```
````

**YAML-style** (recommended for many options):
````markdown
```{figure} image.png
---
name: my-figure
alt: Description
width: 80%
---
Figure caption here
```
````

### Colon Fence Syntax

MyST also supports `:::` fences (requires `colon_fence` extension):

```markdown
:::{note}
This is a note using colon fences.
:::
```

### Nesting Directives

Use additional backticks/colons on outer block:

`````markdown
````{important}
```{note}
Nested content
```
````
`````

Or with colon fences:

```markdown
::::{important}
:::{note}
Nested content
:::
::::
```

## Role Syntax

**Format**: Inline, single-line only

```markdown
{rolename}`content`
```

**Examples**:
- `{ref}`target`` - Cross-reference
- `{doc}`./path`` - Document link
- `{term}`glossary entry`` - Glossary reference
- `{abbr}`MyST (Markedly Structured Text)`` - Abbreviation

### Common Roles

| Role | Purpose | Example |
|------|---------|---------|
| `{ref}` | Cross-reference to target | `{ref}`my-section`` |
| `{numref}` | Numbered reference | `{numref}`fig-%s <my-fig>`` |
| `{eq}` | Equation reference | `{eq}`my-equation`` |
| `{doc}` | Document link | `{doc}`./other-file`` |
| `{download}` | Download link | `{download}`./file.zip`` |
| `{term}` | Glossary term | `{term}`definition`` |

## Target/Anchor Syntax

### Explicit Targets

Place `(label)=` before any content to create a referenceable anchor:

```markdown
(my-section)=
## Section Title

(my-paragraph)=
This paragraph can be referenced.
```

### Directive Labels

Add `:label:` or `:name:` option to directives:

````markdown
```{figure} image.png
:label: my-fig
:name: my-fig

Caption text
```
````

### Math Labels

````markdown
```{math}
:label: my-equation

e = mc^2
```
````

## Standard Admonitions

MyST provides 10 built-in admonition types:

### Informational (Blue)
- `{note}` - General information
- `{important}` - Key information

### Success/Tips (Green)
- `{hint}` - Helpful tips
- `{tip}` - Practical advice
- `{seealso}` - Related resources

### Warning (Orange)
- `{attention}` - Requires focus
- `{caution}` - Potential issues
- `{warning}` - Critical alerts

### Error (Red)
- `{danger}` - Serious risks
- `{error}` - Error conditions

### Admonition Syntax

```markdown
:::{note}
Basic note content
:::

:::{warning} Custom Title
Warning with a custom title
:::

:::{tip}
:class: dropdown
Collapsible tip
:::
```

### Generic Admonition

```markdown
:::{admonition} My Custom Title
:class: tip

Content with tip styling but custom title
:::
```

## Cross-Reference Syntax

### Link to Target

```markdown
[link text](#my-target)
[](#my-target)          <!-- Uses target's title -->
@my-target              <!-- Shorthand syntax -->
```

### Reference Roles

```markdown
{ref}`my-target`
{ref}`custom text <my-target>`
{numref}`Figure %s <my-fig>`
```

## Frontmatter

MyST supports YAML frontmatter:

```yaml
---
title: Document Title
author: Author Name
date: 2024-01-15
tags:
  - tag1
  - tag2
---
```

## AST Node Types

This section documents the MyST AST schema for LSP implementation.

### mystDirective Node

Represents block-level directive content:

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `type` | string | Yes | Always `"mystDirective"` |
| `name` | string | Yes | Directive identifier (e.g., `"note"`) |
| `args` | string | No | Arguments after directive name |
| `options` | object | No | Configuration key-value pairs |
| `value` | string | No | Raw body text excluding options |
| `children` | array | No | Parsed content nodes |

### mystRole Node

Represents inline role content:

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `type` | string | Yes | Always `"mystRole"` |
| `name` | string | Yes | Role identifier (e.g., `"ref"`) |
| `value` | string | No | Raw content text |
| `children` | array | No | Parsed content nodes |

### crossReference Node

Inline reference to associated nodes:

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `type` | string | Yes | Always `"crossReference"` |
| `kind` | string | No | Reference kind: `"eq"`, `"numref"`, or `"ref"` |
| `identifier` | string | Yes | Target identifier |
| `label` | string | No | Display label text |
| `children` | array | No | StaticPhrasingContent nodes |

### admonition Node

Structured callout content:

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `type` | string | Yes | Always `"admonition"` |
| `kind` | string | No | Admonition type (note, warning, etc.) |
| `class` | string | No | CSS class override |
| `children` | array | No | admonitionTitle followed by FlowContent |

### Content Hierarchies

| Category | Description | Examples |
|----------|-------------|----------|
| **FlowContent** | Block-level elements | paragraphs, headings, lists, directives, tables |
| **PhrasingContent** | Inline elements | emphasis, links, roles, footnotes |
| **StaticPhrasingContent** | Non-dynamic inline | text, code, images, inline math |

## Implementation Notes

### Parsing Priority

1. **Frontmatter** - Must be at document start
2. **Targets/Anchors** - `(label)=` patterns
3. **Directives** - Fenced blocks with `{name}`
4. **Roles** - Inline `{name}`content``
5. **Standard Markdown** - CommonMark elements

### AST Characteristics

- AST represents state **immediately after parsing**
- References remain **unresolved** in initial AST
- Directive/role structures persist intact
- Resolution happens in later processing phase

### Regex Patterns for Parsing

**Directive (backtick fence)**:
```text
^(`{3,})\{([a-zA-Z][a-zA-Z0-9_-]*)\}(.*)$
```

**Directive (colon fence)**:
```text
^(:{3,})\{([a-zA-Z][a-zA-Z0-9_-]*)\}(.*)$
```

**Target/Anchor**:
```text
^\(([a-zA-Z][a-zA-Z0-9_-]*)\)=$
```

**Role**:
```text
\{([a-zA-Z][a-zA-Z0-9_-]*)\}`([^`]+)`
```

## Resources

### Official Documentation
- [MyST Specification](https://mystmd.org/spec)
- [MyST Guide](https://mystmd.org/guide)
- [Cross-References Guide](https://mystmd.org/guide/cross-references)
- [Admonitions Guide](https://mystmd.org/guide/admonitions)

### Schema & Test Data
- [JSON Schema](https://unpkg.com/myst-spec@0.0.5/dist/myst.schema.json) - Complete AST schema
- [Test Cases](https://unpkg.com/myst-spec@0.0.5/dist/myst.tests.json) - CommonMark test suite

### Implementations
- [myst-parser (Python/Sphinx)](https://myst-parser.readthedocs.io/)
- [mystmd (JavaScript)](https://mystmd.org/)

```{warning}
The myst-spec AST is still in development. Structures may change without notice.
Always verify against the latest specification.
```
