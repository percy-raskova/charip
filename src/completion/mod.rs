//! Autocomplete providers for MyST Markdown documents.
//!
//! This module implements the LSP `textDocument/completion` capability,
//! providing context-aware suggestions as users type.
//!
//! # Architecture
//!
//! The completion system uses a chain-of-responsibility pattern:
//!
//! ```text
//! get_completions()
//!     ├── MystDirectiveCompleter   (```{directive)
//!     ├── RoleNameCompleter        ({ref, {doc, etc.)
//!     ├── MystRoleCompleter        ({ref}`target`)
//!     ├── MarkdownLinkCompleter    ([text](path))
//!     ├── TagCompleter             (#tag-name)
//!     ├── FootnoteCompleter        ([^footnote])
//!     ├── CalloutCompleter         (> [!note])
//!     └── UnindexedBlockCompleter  (^block-id)
//! ```
//!
//! Each completer is tried in order. The first one that matches the
//! cursor context provides completions.
//!
//! # Trigger Characters
//!
//! Completions are triggered by specific characters:
//!
//! | Trigger | Context | Completer |
//! |---------|---------|-----------|
//! | `{` | Start of MyST role | RoleNameCompleter |
//! | `` ` `` | Inside role target | MystRoleCompleter |
//! | `(` | Inside markdown link | MarkdownLinkCompleter |
//! | `#` | Tag or heading | TagCompleter |
//! | `[` | Link or footnote | FootnoteCompleter |
//! | `>` | Blockquote/callout | CalloutCompleter |
//! | ` ` | Various contexts | Context-dependent |
//!
//! # Writing a Completer
//!
//! Implement the [`Completer`] trait:
//!
//! ```rust,ignore
//! impl<'a> Completer<'a> for MyCompleter {
//!     fn construct(context: Context<'a>, line: usize, char: usize) -> Option<Self> {
//!         // Return Some(self) if cursor is in valid context
//!     }
//!
//!     fn completions(&self) -> Vec<impl Completable<'a, Self>> {
//!         // Return items to show in completion menu
//!     }
//! }
//! ```
//!
//! # Submodules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `link_completer` | Markdown links `[](path)` |
//! | `myst_directive_completer` | MyST directives `{directive}` |
//! | `myst_role_completer` | MyST role targets `{ref}\`target\`` |
//! | `role_name_completer` | MyST role names `{ref`, `{doc` |
//! | `tag_completer` | Tags `#topic` |
//! | `footnote_completer` | Footnotes `[^name]` |
//! | `callout_completer` | Admonitions `> [!note]` |
//! | `matcher` | Fuzzy matching utilities |

use std::path::Path;

use tower_lsp::lsp_types::{CompletionItem, CompletionList, CompletionParams, CompletionResponse};

use crate::{config::Settings, vault::Vault};

use self::callout_completer::CalloutCompleter;
use self::myst_directive_completer::MystDirectiveCompleter;
use self::myst_role_completer::MystRoleCompleter;
use self::role_name_completer::RoleNameCompleter;
use self::{
    footnote_completer::FootnoteCompleter, link_completer::MarkdownLinkCompleter,
    tag_completer::TagCompleter, unindexed_block_completer::UnindexedBlockCompleter,
};

mod callout_completer;
mod footnote_completer;
mod link_completer;
mod matcher;
mod myst_directive_completer;
mod myst_role_completer;
mod role_name_completer;
mod tag_completer;
mod unindexed_block_completer;
mod util;

#[derive(Clone, Copy)]
pub struct Context<'a> {
    vault: &'a Vault,
    path: &'a Path,
    settings: &'a Settings,
}

pub trait Completer<'a>: Sized {
    fn construct(context: Context<'a>, line: usize, character: usize) -> Option<Self>
    where
        Self: Sized + Completer<'a>;

    fn completions(&self) -> Vec<impl Completable<'a, Self>>
    where
        Self: Sized;

    type FilterParams;
    /// Completere like nvim-cmp are odd so manually define the filter text as a situational workaround
    fn completion_filter_text(&self, params: Self::FilterParams) -> String;

    // fn compeltion_resolve(&self, vault: &Vault, resolve_item: CompletionItem) -> Option<CompletionItem>;
}

pub trait Completable<'a, T: Completer<'a>>: Sized {
    fn completions(&self, completer: &T) -> Option<CompletionItem>;
}

/// Range indexes for one line of the file; NOT THE WHOLE FILE
type LineRange<T> = std::ops::Range<T>;

pub fn get_completions(
    vault: &Vault,
    params: &CompletionParams,
    path: &Path,
    config: &Settings,
) -> Option<CompletionResponse> {
    let completion_context = Context {
        vault,
        path,
        settings: config,
    };

    // I would refactor this if I could figure out generic closures
    // MyST directive completion (high priority when in fence context)
    run_completer::<MystDirectiveCompleter>(
        completion_context,
        params.text_document_position.position.line,
        params.text_document_position.position.character,
    )
    // Role name completion ({  -> suggests ref, doc, etc.)
    // High priority - before MystRoleCompleter which handles target completion
    .or_else(|| {
        run_completer::<RoleNameCompleter>(
            completion_context,
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        )
    })
    // MyST role TARGET completion ({ref}`, {doc}`, etc.)
    .or_else(|| {
        run_completer::<MystRoleCompleter>(
            completion_context,
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        )
    })
    .or_else(|| {
        run_completer::<UnindexedBlockCompleter<MarkdownLinkCompleter>>(
            completion_context,
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        )
    })
    .or_else(|| {
        run_completer::<MarkdownLinkCompleter>(
            completion_context,
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        )
    })
    .or_else(|| {
        run_completer::<TagCompleter>(
            completion_context,
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        )
    })
    .or_else(|| {
        run_completer::<FootnoteCompleter>(
            completion_context,
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        )
    })
    .or_else(|| {
        run_completer::<CalloutCompleter>(
            completion_context,
            params.text_document_position.position.line,
            params.text_document_position.position.character,
        )
    })
}

fn run_completer<'a, T: Completer<'a>>(
    context: Context<'a>,
    line: u32,
    character: u32,
) -> Option<CompletionResponse> {
    let completer = T::construct(context, line as usize, character as usize)?;
    let completions = completer.completions();

    let completions = completions
        .into_iter()
        .take(20)
        .flat_map(|completable| {
            completable
                .completions(&completer)
                .into_iter()
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect::<Vec<CompletionItem>>();

    Some(CompletionResponse::List(CompletionList {
        is_incomplete: true,
        items: completions,
    }))
}
