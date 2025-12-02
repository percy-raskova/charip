# API Reference

(api-docs)=
## Overview

The charip LSP implements the Language Server Protocol 3.17.

See {doc}`../guides/getting-started` for installation instructions.

## Methods

The following LSP methods are implemented:

### textDocument/completion

Provides completions for:
- {term}`Directive` names after triple backticks
- {term}`Role` targets after role prefix
- {term}`Anchor` targets for {ref} roles
- Glossary terms for {term} roles

### textDocument/definition

Go-to-definition for:
- {ref}`installation-anchor` - MyST anchors
- [Markdown links](../guides/getting-started.md#installation)
- {term}`MyST` glossary terms

### textDocument/references

Find all references to:
- Anchors like {ref}`config-anchor`
- Files like {doc}`../glossary`
- Headings

### textDocument/rename

Rename symbols:
- Rename {ref}`installation-anchor` from definition or reference site
- Updates all references across the vault

^api-methods

## Downloading Assets

Example downloadable file: {download}`../assets/example.zip`

## Related

- {ref}`config-note` for configuration
- {doc}`equations` for math support

#api #reference #lsp
