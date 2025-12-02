# Configuration

(config-anchor)=
## Configuration Options

The LSP can be configured via a config file or editor settings.

```{note}
:name: config-note

Configuration is optional. The LSP works with sensible defaults.
```

### Settings File

Create `.charip.toml` in your vault root:

```toml
[diagnostics]
unresolved_links = true
```

^config-example

### Editor Configuration

```{warning}
:name: editor-warning

Each editor has its own configuration format. See your editor's
documentation for LSP client setup.
```

## Related

- {ref}`installation-anchor` for initial setup
- {doc}`../reference/api` for programmatic usage
- {term}`Directive` syntax reference

## Link Reference Definitions

For external documentation, see [sphinx] and [myst-parser].

[sphinx]: https://www.sphinx-doc.org/
[myst-parser]: https://myst-parser.readthedocs.io/

#configuration #settings
