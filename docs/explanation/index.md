---
title: Understanding charip-lsp
---

# Understanding charip-lsp

This section explains how charip-lsp works and the design decisions behind it.

## Overview

charip-lsp is a Language Server Protocol implementation specialized for MyST documentation. It provides editor-agnostic intelligence by maintaining an in-memory model of your documentation project.

## Key Concepts

### The Vault

The "vault" is charip-lsp's in-memory representation of your documentation project. It contains:

- All markdown files, parsed into structured data
- A graph of relationships between documents
- Indexes for fast lookups

### References and Referenceables

charip-lsp models documentation as a graph:

References
: Things that point to other things (links, roles)

Referenceables
: Things that can be pointed to (anchors, headings, files)

This abstraction enables consistent handling of navigation, renaming, and diagnostics across different syntax types.

### Graph-Based Architecture

Unlike simpler approaches that scan files on each request, charip-lsp maintains a persistent graph structure. This enables:

- O(1) lookups instead of O(n) scans
- Efficient backlink queries
- Cycle detection for includes
- Orphan document detection

## In This Section

{doc}`architecture`
: The overall structure of the codebase and how components interact.

{doc}`graph-model`
: Deep dive into the petgraph-based document graph.

## Design Principles

### Performance First

charip-lsp is built in Rust with performance as a core requirement:

- Parallel file parsing with Rayon
- Rope data structures for efficient text manipulation
- Graph-based storage for O(1) queries

### MyST-Native

Rather than treating MyST as "Markdown with extras," charip-lsp understands MyST constructs natively:

- Roles and directives are first-class concepts
- Anchors are tracked as explicit targets
- Frontmatter substitutions are validated

### Editor Agnostic

charip-lsp implements the Language Server Protocol, working with any compatible editor. The same binary powers Neovim, VS Code, Helix, and others.
