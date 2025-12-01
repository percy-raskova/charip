---
title: charip-lsp Documentation
---

# charip-lsp

A Language Server for MyST (Markedly Structured Text) documents.

## About the Name

**Charip** (자립, *ja-rip*) means "self-reliance" or "economic self-sufficiency" in Korean. It is one of the three pillars of the Juche idea, which holds that true independence requires building one's own infrastructure rather than depending on external systems.

This project embodies that principle: rather than adapting to tools built for other purposes, we build tools that serve our specific needs. Revolutionary movements need revolutionary infrastructure—this LSP is one component of that effort.

## Project Status

```{admonition} Current Phase
:class: tip

**Phase 1 Complete**: MyST directive and anchor parsing integrated into the vault.

**Phase 2 Next**: Graph architecture using `petgraph` for cross-file reference resolution.
```

## Documentation

```{toctree}
:maxdepth: 2
:caption: Architecture

architecture/overview
architecture/data-model
architecture/deep-dive
```

```{toctree}
:maxdepth: 2
:caption: Development

development/myst-implementation
development/future-enhancements
development/parser-analysis
development/target-environment
```

```{toctree}
:maxdepth: 2
:caption: Reference

reference/myst-spec
reference/features
reference/configuration
reference/testing
```

## Quick Links

- **Source**: [GitHub](https://github.com/user/charip-lsp)
- **Upstream**: [markdown-oxide](https://github.com/Feel-ix-343/markdown-oxide)
- **MyST Spec**: [MyST Documentation](https://myst-parser.readthedocs.io/)
