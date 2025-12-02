# Glossary

```{glossary}
API
  Application Programming Interface. A set of protocols and tools
  for building software applications.

MyST
  Markedly Structured Text. An extended Markdown syntax used by
  Sphinx documentation systems.

LSP
  Language Server Protocol. A protocol for providing IDE features
  like autocomplete, go-to-definition, and diagnostics.

Anchor
  A named target in a document that can be referenced using {ref}
  roles. Created with `(anchor-name)=` syntax.

Directive
  A MyST block-level extension using triple-backtick fence syntax.
  Examples include note, warning, code-block, and glossary.

Role
  A MyST inline extension using `{role}`content`` syntax.
  Examples include ref, doc, term, and eq.
```

## Using Glossary Terms

Reference terms with {term}`API` or {term}`MyST` roles.

The {term}`LSP` provides features like go-to-definition for {term}`Anchor` targets.

{term}`Directive` and {term}`Role` are the two extension mechanisms in MyST.
