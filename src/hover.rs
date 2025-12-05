//! Hover provider for MyST Markdown documents.
//!
//! This module implements the LSP `textDocument/hover` capability,
//! showing contextual information when hovering over references.
//!
//! # Hover Targets
//!
//! | Target | Shows |
//! |--------|-------|
//! | Markdown link | Target file preview (first lines) |
//! | Heading link | Heading text and context |
//! | MyST role | Target anchor/heading preview |
//! | Tag | List of files with the same tag |
//!
//! # Configuration
//!
//! Hover can be disabled via [`Settings::hover`]:
//!
//! ```json
//! { "hover": false }
//! ```

use std::path::Path;

use tower_lsp::lsp_types::{Hover, HoverContents, HoverParams};

use crate::{config::Settings, ui::preview_reference, vault::Vault};

/// Generate hover content for the element at the cursor position.
///
/// Returns a preview of the reference target when hovering over:
/// - Markdown links (`[text](path.md)`)
/// - MyST roles (`{ref}\`target\``)
/// - Tags (`#topic`)
///
/// # Arguments
///
/// * `vault` - The indexed vault
/// * `params` - LSP hover request parameters
/// * `path` - Path to the current file
/// * `settings` - Configuration (checks `settings.hover`)
///
/// # Returns
///
/// `Some(Hover)` with markdown content, or `None` if:
/// - Hover is disabled in settings
/// - Cursor is not on a reference
/// - Reference has no previewable target
pub fn hover(
    vault: &Vault,
    params: &HoverParams,
    path: &Path,
    settings: &Settings,
) -> Option<Hover> {
    if !settings.hover {
        return None;
    }

    let cursor_position = params.text_document_position_params.position;

    match (
        vault.select_reference_at_position(path, cursor_position),
        vault.select_referenceable_at_position(path, cursor_position),
    ) {
        (Some(reference), _) => preview_reference(vault, path, reference).map(|markup| Hover {
            contents: HoverContents::Markup(markup),
            range: None,
        }),
        _ => None,
    }
}
