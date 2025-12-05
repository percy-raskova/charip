---
title: charip-lsp Documentation
---

# charip-lsp

A Language Server Protocol implementation for [MyST](inv:myst#index) (Markedly Structured Text) documentation projects.

## What is charip-lsp?

charip-lsp brings IDE-quality features to MyST documentation:

- **Intelligent autocomplete** for directives, roles, cross-references, and glossary terms
- **Go-to-definition** that understands `{ref}`, `{doc}`, `{term}`, and anchors
- **Find references** to see everywhere a target is used
- **Real-time diagnostics** for broken links, missing images, and circular includes
- **Rename refactoring** that updates all references automatically

Built in Rust for performance, charip-lsp uses a graph-based architecture that scales to large documentation projects.

```{admonition} Project Status
:class: tip

**Production ready.** All core MyST features are implemented with 347 tests passing.
```

## Quick Example

```{code-block} markdown
:caption: MyST with full LSP support

(installation-guide)=
# Installation Guide

See the {ref}`configuration-options` section for details.

For API documentation, check {doc}`/reference/api`.

The {term}`Language Server Protocol` enables editor integration.
```

With charip-lsp, every reference in this example gets:
- Autocomplete suggestions as you type
- Click-to-navigate to the target
- Rename support that updates all usages
- Warnings if targets don't exist

## Documentation

```{toctree}
:maxdepth: 2
:caption: Getting Started

getting-started/index
getting-started/installation
getting-started/editor-setup
```

```{toctree}
:maxdepth: 2
:caption: User Guide

user-guide/index
user-guide/autocomplete
user-guide/navigation
user-guide/cross-references
user-guide/diagnostics
```

```{toctree}
:maxdepth: 2
:caption: Reference

reference/index
reference/capabilities
reference/myst-syntax
reference/configuration
reference/cli
```

```{toctree}
:maxdepth: 2
:caption: Understanding charip-lsp

explanation/index
explanation/architecture
explanation/graph-model
```

```{toctree}
:maxdepth: 2
:caption: Contributing

contributing/index
contributing/development
contributing/testing
contributing/commit-philosophy
```

## About the Name

**Charip** (자립, *ja-rip*) is the Korean word for "self-reliance." The name reflects the project's philosophy: rather than adapting tools designed for other workflows, technical documentation deserves purpose-built infrastructure that understands its specific needs.

## Links

- [Source Code](https://github.com/user/charip-lsp)
- [MyST Parser Documentation](inv:myst#index)
- [Language Server Protocol Specification](https://microsoft.github.io/language-server-protocol/)
