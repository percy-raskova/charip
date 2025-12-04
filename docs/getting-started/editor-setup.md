---
title: Editor Setup
---

# Editor Setup

charip-lsp works with any editor that supports the Language Server Protocol. This guide covers the most common configurations.

## Neovim

### Using nvim-lspconfig

Add charip-lsp to your Neovim configuration:

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

-- Register charip-lsp if not already defined
if not configs.charip then
  configs.charip = {
    default_config = {
      cmd = { 'charip' },
      filetypes = { 'markdown' },
      root_dir = lspconfig.util.root_pattern('conf.py', '.git', '_toc.yml'),
      settings = {},
    },
  }
end

-- Enable the server
lspconfig.charip.setup({
  on_attach = function(client, bufnr)
    -- Your on_attach function
  end,
})
```

### Key Mappings

Recommended mappings for MyST documentation:

```lua
vim.keymap.set('n', 'gd', vim.lsp.buf.definition, { desc = 'Go to definition' })
vim.keymap.set('n', 'gr', vim.lsp.buf.references, { desc = 'Find references' })
vim.keymap.set('n', 'K', vim.lsp.buf.hover, { desc = 'Hover documentation' })
vim.keymap.set('n', '<leader>rn', vim.lsp.buf.rename, { desc = 'Rename symbol' })
vim.keymap.set('n', '<leader>ca', vim.lsp.buf.code_action, { desc = 'Code actions' })
```

### Telescope Integration

For fuzzy finding symbols:

```lua
vim.keymap.set('n', '<leader>ds', require('telescope.builtin').lsp_document_symbols,
  { desc = 'Document symbols' })
vim.keymap.set('n', '<leader>ws', require('telescope.builtin').lsp_workspace_symbols,
  { desc = 'Workspace symbols' })
```

## VS Code

### Extension Installation

1. Install the charip-lsp VS Code extension from the marketplace (coming soon)

Or manually:

1. Clone the repository
2. Build the extension:
   ```bash
   cd vscode-extension
   npm install
   npm run compile
   ```
3. Package and install:
   ```bash
   npx vsce package
   code --install-extension charip-lsp-*.vsix
   ```

### Configuration

In your VS Code settings (`settings.json`):

```json
{
  "charip-lsp.serverPath": "/path/to/charip",
  "charip-lsp.trace.server": "verbose"
}
```

## Other Editors

charip-lsp uses standard LSP over stdio, so it works with any LSP-capable editor.

### Generic Configuration

| Setting | Value |
|---------|-------|
| Command | `charip` |
| Transport | stdio |
| File types | `markdown`, `md` |
| Root markers | `conf.py`, `.git`, `_toc.yml` |

### Helix

In `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "markdown"
language-server = { command = "charip" }
roots = ["conf.py", ".git"]
```

### Emacs (lsp-mode)

```elisp
(use-package lsp-mode
  :config
  (add-to-list 'lsp-language-id-configuration '(markdown-mode . "markdown"))
  (lsp-register-client
    (make-lsp-client
      :new-connection (lsp-stdio-connection '("charip"))
      :major-modes '(markdown-mode)
      :server-id 'charip)))
```

## Verifying the Setup

Once configured, open a `.md` file in your documentation project. You should see:

1. **Diagnostics** - Warnings for broken references appear inline
2. **Completions** - Type `{ref}\`` to see anchor suggestions
3. **Hover** - Hover over a reference to see its target

If features aren't working, check your editor's LSP logs for error messages.

## Next Steps

Now that your editor is configured, learn how to use charip-lsp features in the {doc}`/user-guide/index`.
