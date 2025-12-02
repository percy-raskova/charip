---
title: charip-lsp Documentation
---

# charip-lsp

A Language Server for MyST (Markedly Structured Text) documents.

## About the Name

**Charip** (자립, *ja-rip*) means "self-reliance" or "economic self-sufficiency" in Korean. It is one of the three pillars of the Juche idea, which holds that true independence requires building one's own infrastructure rather than depending on external systems.

This project embodies that principle: rather than adapting PKM tools built for Obsidian workflows, MyST documentation writers build their own infrastructure. Technical documentation deserves tooling that understands its specific needs—cross-references, glossaries, directives—without the ideological assumptions of note-taking systems designed for different purposes.

## Project Status

```{admonition} Current Status
:class: tip

All core MyST features implemented:
- Directive/role/anchor extraction
- Go-to-definition and find references
- Directive and role target autocomplete
- Anchor rename from roles
- Glossary term completion
- Broken reference diagnostics

**194 tests passing**
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
