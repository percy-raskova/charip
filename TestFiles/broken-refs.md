# Broken References

This file contains intentionally broken references for diagnostics testing.
Each broken reference should produce a diagnostic from the LSP.

## Broken Document References

- {doc}`nonexistent-file`
- {doc}`guides/missing-guide`
- {doc}`wrong/path/to/file`

## Broken Anchor References

- {ref}`nonexistent-anchor`
- {ref}`typo-in-anchor-name`
- {ref}`missing-target`

## Broken Glossary References

- {term}`UndefinedTerm`
- {term}`NotInGlossary`
- {term}`MissingDefinition`

## Broken Equation References

- {eq}`missing-equation`
- {eq}`no-such-label`

## Broken Markdown Links

- [Missing File](does-not-exist.md)
- [Wrong Path](wrong/path.md)
- [Bad Heading](index.md#nonexistent-heading)
- [Missing Block](index.md#^no-such-block)

## Broken Download

- {download}`assets/missing-file.zip`

## Valid References (for comparison)

These should NOT produce diagnostics:
- {doc}`index`
- {ref}`installation-anchor`
- {term}`API`
- [Index](index.md)

#broken #diagnostics #testing
