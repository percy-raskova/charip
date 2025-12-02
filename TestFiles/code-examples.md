# Code Examples

This file tests that roles inside code blocks are NOT parsed.

## Python Example

```python
# This comment mentions {ref}`should-not-parse` but it's in a code block
def example():
    """
    Documentation with {doc}`fake-reference` that should be ignored.
    Also {term}`NotReal` should not trigger diagnostics.
    """
    pass
```

## Markdown Example

````markdown
# Example Document

Here's how to use roles:

- {ref}`example-anchor` for anchor references
- {doc}`example-doc` for document references
- {term}`example-term` for glossary terms
````

## Shell Example

```bash
# Deploy with {ref}`deploy-anchor` configuration
echo "This {doc}`should-not-parse` is in a code block"
```

## Inline Code

Inline code like `{ref}`also-ignored`` should not be parsed as a role.

The syntax `{doc}`not-a-reference`` in backticks is documentation, not a link.

## Actual Role Outside Code Block

This {ref}`installation-anchor` IS a real reference and should work.

Same with {doc}`index` and {term}`API`.

#code #examples #exclusion
