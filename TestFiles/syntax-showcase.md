# MyST Syntax Showcase

This file demonstrates ALL MyST syntax elements from myst.schema.json.

## Inline Formatting

Basic **strong** and *emphasis* text with `inline code`.

Subscript H{sub}`2`O and superscript x{sup}`2`.

{u}`Underlined text` for emphasis.

{abbr}`HTML (HyperText Markup Language)` with abbreviation.

## Inline Math

The quadratic formula is $x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}$.

Einstein's famous equation: $E = mc^2$.

## Images

![Example Image](assets/example.zip "Example tooltip")

## Blockquote

> This is a blockquote demonstrating the Blockquote node type.
> It can span multiple lines.
>
> And contain multiple paragraphs.

## Thematic Break

Content above the break.

---

Content below the break.

## MyST Comments

% This is a MyST comment - it will not appear in rendered output.
% Comments are useful for leaving notes in the source.

Visible text after the comment.

## MyST Block Breaks

First block of content.

+++

Second block of content (separated by block break).

+++{"cell_type": "code"}

Third block with metadata.

## GFM Tables

| Feature | Status | Notes |
|---------|--------|-------|
| Tables | Supported | GFM extension |
| Alignment | Left | Default |
| Headers | Yes | First row |

| Right | Center | Left |
|------:|:------:|:-----|
| 100 | Yes | Data |
| 200 | No | More |

## All Admonition Types

```{attention}
This is an attention admonition.
```

```{caution}
This is a caution admonition.
```

```{danger}
This is a danger admonition.
```

```{error}
This is an error admonition.
```

```{hint}
This is a hint admonition.
```

```{important}
This is an important admonition.
```

```{seealso}
This is a seealso admonition.
```

```{tip}
This is a tip admonition.
```

```{warning}
This is a warning admonition (also in configuration.md).
```

```{note}
This is a note admonition (also in configuration.md).
```

## Admonition with Custom Title

```{admonition} Custom Title Here
:class: tip

This admonition has a custom title instead of the default.
```

## Figure with Caption and Legend

```{figure} assets/example.zip
:name: syntax-figure
:width: 50%
:align: center

This is the **caption** for the figure.

+++
This is the legend providing additional context about the figure.
It can contain *formatted* text and {term}`MyST` roles.
```

## Definition List

Term 1
: Definition for term 1.

Term 2
: Definition for term 2.
: Can have multiple definitions.

Complex Term
: A definition that contains
  multiple lines and even

  multiple paragraphs.

## Task List

- [x] Completed task
- [ ] Incomplete task
- [x] Another done item

## Substitutions

{{project_name}} is a placeholder for substitution.

## Cross References

- Reference the figure: {numref}`syntax-figure`
- Reference an anchor: {ref}`installation-anchor`
- Reference a document: {doc}`index`

#syntax #showcase #comprehensive
