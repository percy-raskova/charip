//! Configuration management for charip-lsp.
//!
//! Settings are loaded from multiple sources in priority order:
//! 1. `~/.config/moxide/settings` (global)
//! 2. `<vault>/.moxide` (vault-specific)
//!
//! Vault-specific settings override global settings.

use std::path::Path;

use anyhow::anyhow;
use config::{Config, File};
use serde::Deserialize;
use tower_lsp::lsp_types::ClientCapabilities;

/// LSP server configuration options.
///
/// Controls various features of the language server including completions,
/// diagnostics, hover information, and parsing behavior.
///
/// # Configuration Files
///
/// Settings are loaded from TOML files:
/// - Global: `~/.config/moxide/settings`
/// - Per-vault: `<vault_root>/.moxide`
///
/// # Example Configuration
///
/// ```toml
/// # .moxide
/// dailynote = "%Y-%m-%d"
/// heading_completions = true
/// unresolved_diagnostics = true
/// case_matching = "Smart"
/// frontmatter_schema_path = "_schemas/frontmatter.schema.json"
/// ```
///
/// # Default Values
///
/// All settings have sensible defaults. See [`Default`] implementation.
#[derive(Deserialize, Debug, Clone)]
pub struct Settings {
    /// Date format for daily note filenames.
    ///
    /// Uses strftime format specifiers (e.g., `%Y-%m-%d` for `2024-03-15`).
    /// Default: `%Y-%m-%d`
    pub dailynote: String,

    /// Folder for new files created via code actions.
    ///
    /// Relative to vault root. Empty string uses vault root.
    /// Default: `""`
    pub new_file_folder_path: String,

    /// Folder containing daily notes.
    ///
    /// Relative to vault root. Default: `""`
    pub daily_notes_folder: String,

    /// Enable heading-based completions.
    ///
    /// When true, typing `#` suggests headings from the vault.
    /// Default: `true`
    pub heading_completions: bool,

    /// Use first heading as document title in completions.
    ///
    /// When true, `# Title` becomes the display name instead of filename.
    /// Default: `true`
    pub title_headings: bool,

    /// Report diagnostics for unresolved references.
    ///
    /// When true, broken links show as warnings/errors.
    /// Default: `true`
    pub unresolved_diagnostics: bool,

    /// Enable semantic token highlighting.
    ///
    /// Provides rich syntax highlighting via LSP semantic tokens.
    /// May be disabled if editor doesn't support it.
    /// Default: `true` (disabled if client lacks capability)
    pub semantic_tokens: bool,

    /// Parse tags inside code blocks.
    ///
    /// When true, `#tag` inside ``` blocks is recognized.
    /// Default: `false`
    pub tags_in_codeblocks: bool,

    /// Parse references inside code blocks.
    ///
    /// When true, links inside ``` blocks are indexed.
    /// Default: `false`
    pub references_in_codeblocks: bool,

    /// Include `.md` extension in markdown link completions.
    ///
    /// When true: `[text](file.md)`. When false: `[text](file)`.
    /// Default: `false`
    pub include_md_extension_md_link: bool,

    /// Enable hover information.
    ///
    /// Shows previews and metadata when hovering over links.
    /// Default: `true`
    pub hover: bool,

    /// Case sensitivity for fuzzy matching.
    ///
    /// - `Ignore`: Case-insensitive matching
    /// - `Smart`: Case-sensitive only if query has uppercase
    /// - `Respect`: Always case-sensitive
    ///
    /// Default: `Smart`
    pub case_matching: Case,

    /// Enable inlay hints.
    ///
    /// Shows inline annotations (e.g., resolved link targets).
    /// Default: `true`
    pub inlay_hints: bool,

    /// Enable block transclusion in hover.
    ///
    /// Shows embedded block content on hover.
    /// Default: `true`
    pub block_transclusion: bool,

    /// Length of transcluded blocks in hover.
    ///
    /// - `Full`: Show entire block
    /// - `Partial(n)`: Show first n characters
    ///
    /// Default: `Full`
    pub block_transclusion_length: EmbeddedBlockTransclusionLength,

    /// Show only filenames in link completions (not full paths).
    ///
    /// Default: `false`
    pub link_filenames_only: bool,

    /// Path to JSON Schema for frontmatter validation.
    ///
    /// Relative to vault root. If file doesn't exist, validation is disabled.
    /// Default: `_schemas/frontmatter.schema.json`
    pub frontmatter_schema_path: String,
}

/// Case sensitivity mode for fuzzy matching.
#[derive(Clone, Debug, Deserialize)]
pub enum Case {
    /// Always case-insensitive
    Ignore,
    /// Case-sensitive only if query contains uppercase
    Smart,
    /// Always case-sensitive
    Respect,
}

/// How much of an embedded block to show in hover previews.
#[derive(Clone, Debug, Deserialize)]
pub enum EmbeddedBlockTransclusionLength {
    /// Show only the first `n` characters
    Partial(usize),
    /// Show the entire block content
    Full,
}

impl Settings {
    pub fn new(root_dir: &Path, capabilities: &ClientCapabilities) -> anyhow::Result<Settings> {
        let expanded = shellexpand::tilde("~/.config/moxide/settings");
        let settings = Config::builder()
            .add_source(File::with_name(&expanded).required(false))
            .add_source(
                File::with_name(&format!(
                    "{}/.moxide",
                    root_dir
                        .to_str()
                        .ok_or(anyhow!("Can't convert root_dir to str"))?
                ))
                .required(false),
            )
            .set_default("new_file_folder_path", "")?
            .set_default("daily_notes_folder", "")?
            .set_default("dailynote", "%Y-%m-%d")?
            .set_default("heading_completions", true)?
            .set_default("unresolved_diagnostics", true)?
            .set_default("title_headings", true)?
            .set_default("semantic_tokens", true)?
            .set_default("tags_in_codeblocks", false)?
            .set_default("references_in_codeblocks", false)?
            .set_default("include_md_extension_md_link", false)?
            .set_default("hover", true)?
            .set_default("case_matching", "Smart")?
            .set_default("inlay_hints", true)?
            .set_default("block_transclusion", true)?
            .set_default("block_transclusion_length", "Full")?
            .set_override_option(
                "semantic_tokens",
                capabilities.text_document.as_ref().and_then(|it| {
                    match it.semantic_tokens.is_none() {
                        true => Some(false),
                        false => None,
                    }
                }),
            )?
            .set_default("link_filenames_only", false)?
            .set_default(
                "frontmatter_schema_path",
                "_schemas/frontmatter.schema.json",
            )?
            .build()
            .map_err(|err| anyhow!("Build err: {err}"))?;

        let settings = settings.try_deserialize::<Settings>()?;

        anyhow::Ok(settings)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            dailynote: "%Y-%m-%d".to_string(),
            new_file_folder_path: "".to_string(),
            daily_notes_folder: "".to_string(),
            heading_completions: true,
            title_headings: true,
            unresolved_diagnostics: true,
            semantic_tokens: false,
            tags_in_codeblocks: false,
            references_in_codeblocks: false,
            include_md_extension_md_link: false,
            hover: true,
            case_matching: Case::Smart,
            inlay_hints: false,
            block_transclusion: true,
            block_transclusion_length: EmbeddedBlockTransclusionLength::Full,
            link_filenames_only: false,
            frontmatter_schema_path: "_schemas/frontmatter.schema.json".to_string(),
        }
    }
}
