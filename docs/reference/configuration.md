---
title: Configuration
---

# Configuration & Settings

Configuration is handled in `src/config.rs`. The system prioritizes local project settings over global settings and attempts to auto-configure based on an existing Obsidian vault.

## Config Priority

1. **Environment Variables** (Implicit via `config` crate)
2. **Local Config**: `.moxide.toml` in the project root
3. **Global Config**: `~/.config/moxide/settings.toml`
4. **Defaults**: Hardcoded defaults + Imports from Obsidian

## The Settings Struct

```rust
pub struct Settings {
    pub dailynote: String,                  // Date format (e.g., "%Y-%m-%d")
    pub new_file_folder_path: String,       // Path for new files
    pub daily_notes_folder: String,         // Path for daily notes
    pub heading_completions: bool,
    pub title_headings: bool,               // Use first H1 as link display text
    pub unresolved_diagnostics: bool,
    pub semantic_tokens: bool,
    pub tags_in_codeblocks: bool,
    pub references_in_codeblocks: bool,
    pub include_md_extension_md_link: bool,
    pub include_md_extension_wikilink: bool,
    pub hover: bool,
    pub case_matching: Case,                // Ignore | Smart | Respect
    pub inlay_hints: bool,
    pub block_transclusion: bool,           // ![[link]] preview via inlay hints
    pub block_transclusion_length: EmbeddedBlockTransclusionLength,
    pub link_filenames_only: bool,
}
```

## Obsidian Import Logic

The LSP looks for `.obsidian/` configuration files to smooth the transition for Obsidian users.

### Daily Notes

Reads `.obsidian/daily-notes.json`:

Folder
: Imports `folder` setting

Format
: Imports `format` (Moment.js) and converts to `chrono` format

```{list-table} Moment.js to Chrono Conversion
:header-rows: 1

* - Moment.js
  - Chrono
* - `YYYY-MM-DD`
  - `%Y-%m-%d`
* - `DD MMMM YYYY`
  - `%d %B %Y`
```

### New File Location

Reads `.obsidian/app.json`:
- Checks `newFileLocation`
- If set to `folder`, imports `newFileFolderPath`

## Key Configuration Files

| File | Purpose |
|------|---------|
| `src/config.rs` | Parsing logic |
| `src/cli.rs` | Uses settings for CLI path resolution |
| `.moxide.toml` | Local project configuration |
