---
title: Configuration
---

# Configuration

charip-lsp can be configured through project-local or global configuration files.

## Configuration Files

Configuration is loaded in order of precedence:

1. **Project config**: `.moxide.toml` in the workspace root
2. **Global config**: `~/.config/moxide/settings.toml`
3. **Obsidian import**: Settings from `.obsidian/` (if present)
4. **Defaults**: Built-in defaults

Later sources fill in missing values; they don't override.

## Configuration Options

### Daily Notes

```toml
# Date format for daily note filenames (chrono format)
dailynote = "%Y-%m-%d"

# Folder for daily notes (relative to vault root)
daily_notes_folder = "daily"
```

### New Files

```toml
# Where to create new files from code actions
new_file_folder_path = ""
```

### Completions

```toml
# Include heading completions in link suggestions
heading_completions = true

# Use first H1 as display text for file links
title_headings = true

# Case sensitivity: "ignore", "smart", or "respect"
case_matching = "smart"
```

### Diagnostics

```toml
# Report unresolved references as diagnostics
unresolved_diagnostics = true
```

### Links

```toml
# Include .md extension in markdown link completions
include_md_extension_md_link = true

# Show only filenames (not paths) in completions
link_filenames_only = false
```

### Features

```toml
# Enable hover previews
hover = true

# Enable inlay hints
inlay_hints = false

# Enable semantic tokens (syntax highlighting)
semantic_tokens = false
```

### Code Blocks

```toml
# Parse tags inside code blocks
tags_in_codeblocks = false

# Parse references inside code blocks
references_in_codeblocks = false
```

## Example Configuration

```{code-block} toml
:caption: .moxide.toml

# Daily notes
dailynote = "%Y-%m-%d"
daily_notes_folder = "journal"

# Completions
heading_completions = true
title_headings = true
case_matching = "smart"

# Diagnostics
unresolved_diagnostics = true

# Links
include_md_extension_md_link = false
```

## Obsidian Compatibility

If you're migrating from Obsidian, charip-lsp can import some settings.

### Daily Notes Import

Reads `.obsidian/daily-notes.json`:

| Obsidian Setting | charip-lsp Setting |
|------------------|-------------------|
| `folder` | `daily_notes_folder` |
| `format` | `dailynote` (converted from Moment.js) |

### Format Conversion

Moment.js formats are converted to chrono:

| Moment.js | chrono |
|-----------|--------|
| `YYYY` | `%Y` |
| `MM` | `%m` |
| `DD` | `%d` |
| `MMMM` | `%B` |
| `dddd` | `%A` |

### App Settings Import

Reads `.obsidian/app.json`:

| Obsidian Setting | charip-lsp Setting |
|------------------|-------------------|
| `newFileFolderPath` | `new_file_folder_path` |

## Editor-Specific Settings

Some settings may be configured in your editor rather than `.moxide.toml`.

### VS Code

```json
{
  "charip-lsp.serverPath": "/path/to/charip",
  "charip-lsp.trace.server": "verbose"
}
```

### Neovim

```lua
lspconfig.charip.setup({
  settings = {
    -- Settings passed to the server
  }
})
```

## Troubleshooting

### Settings Not Applied

1. Ensure `.moxide.toml` is in the workspace root
2. Check file syntax (TOML format)
3. Restart the LSP server after changes

### Finding Active Config

Use `charip config` to open the configuration file the server is using.
