---
title: User Guide
---

# User Guide

This section explains how to use charip-lsp features effectively in your daily documentation workflow.

## Overview

charip-lsp enhances your MyST documentation experience with:

- Smart autocomplete that understands your project structure
- Navigation features to quickly jump between documents
- Cross-reference validation to catch broken links early
- Refactoring tools to safely rename targets

## In This Section

{doc}`autocomplete`
: Master the autocomplete system for directives, roles, and references.

{doc}`navigation`
: Learn to navigate your documentation with go-to-definition and find references.

{doc}`cross-references`
: Work effectively with MyST cross-references (`{ref}`, `{doc}`, `{term}`).

{doc}`diagnostics`
: Understand and resolve warnings and errors from the LSP.

## Workflow Tips

### Start with Structure

Create your anchors and headings first, then reference them. charip-lsp will autocomplete targets as you type.

### Use Consistent Naming

Anchor names like `installation-guide` are easier to remember than `install-1`. The autocomplete helps, but clear names make your documentation more maintainable.

### Check Diagnostics Before Committing

Run through your editor's diagnostics panel to catch broken references before they reach your readers.
