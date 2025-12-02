use std::path::Path;

use anyhow::anyhow;
use config::{Config, File};
use serde::Deserialize;
use tower_lsp::lsp_types::ClientCapabilities;

#[derive(Deserialize, Debug, Clone)]
pub struct Settings {
    /// Format of daily notes
    pub dailynote: String,
    /// Diffrent pages path than default
    pub new_file_folder_path: String,
    pub daily_notes_folder: String,
    pub heading_completions: bool,
    pub title_headings: bool,
    pub unresolved_diagnostics: bool,
    pub semantic_tokens: bool,
    pub tags_in_codeblocks: bool,
    pub references_in_codeblocks: bool,
    pub include_md_extension_md_link: bool,
    pub hover: bool,
    pub case_matching: Case,
    pub inlay_hints: bool,
    pub block_transclusion: bool,
    pub block_transclusion_length: EmbeddedBlockTransclusionLength,
    pub link_filenames_only: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub enum Case {
    Ignore,
    Smart,
    Respect,
}

#[derive(Clone, Debug, Deserialize)]
pub enum EmbeddedBlockTransclusionLength {
    Partial(usize),
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
        }
    }
}
