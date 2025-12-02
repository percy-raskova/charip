//! MyST Role Completer
//!
//! Provides autocomplete for MyST role targets after typing `{ref}`` or `{doc}`` etc.
//!
//! ## Trigger Patterns
//! - `{ref}\`` or `{ref}`partial`
//! - `{doc}\`` or `{doc}`./path`
//! - `{numref}\`` or `{numref}`partial`
//!
//! ## Completion Sources
//! - `{ref}` and `{numref}` -> MystAnchors + Headings
//! - `{doc}` -> Document/file paths

use once_cell::sync::Lazy;
use regex::Regex;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Position, Range, TextEdit,
};

use crate::vault::{get_relative_ref_path, Referenceable, Vault};

use super::{Completable, Completer, Context};

/// The type of MyST role being completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleType {
    /// {ref}`target` - cross-reference to anchor or heading
    Ref,
    /// {numref}`target` - numbered reference to anchor or heading
    NumRef,
    /// {doc}`path` - document/file reference
    Doc,
    /// {download}`path` - downloadable file reference
    Download,
    /// {term}`glossary-term` - glossary term reference
    Term,
}

impl RoleType {
    /// Returns the role name as it appears in MyST syntax.
    pub fn name(&self) -> &'static str {
        match self {
            RoleType::Ref => "ref",
            RoleType::NumRef => "numref",
            RoleType::Doc => "doc",
            RoleType::Download => "download",
            RoleType::Term => "term",
        }
    }
}

/// Completer for MyST role targets.
///
/// Activates when the cursor is positioned inside a role's backticks,
/// for example: `{ref}`my-| ` where `|` is the cursor.
pub struct MystRoleCompleter<'a> {
    /// The type of role being completed
    role_type: RoleType,
    /// The partial target typed so far (may be empty)
    partial_target: String,
    /// Line number in the document
    line: u32,
    /// Character position where the target starts (after the backtick)
    target_start: u32,
    /// Current cursor position
    character: u32,
    /// Reference to the vault for completion lookups
    vault: &'a Vault,
    /// Path of the current file
    path: &'a std::path::Path,
}

impl<'a> Completer<'a> for MystRoleCompleter<'a> {
    fn construct(context: Context<'a>, line: usize, character: usize) -> Option<Self>
    where
        Self: Sized + Completer<'a>,
    {
        let line_chars = context.vault.select_line(context.path, line as isize)?;
        let line_string = String::from_iter(line_chars);

        // Pattern: {role}`partial_target
        // where role is one of: ref, numref, doc, download, term
        // We match up to the cursor position, target may be incomplete
        static ROLE_TARGET_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\{(?<role>ref|numref|doc|download|term)\}`(?<target>[^`]*)$").unwrap()
        });

        // Get the text up to the cursor position
        let text_before_cursor = if character <= line_string.len() {
            &line_string[..character]
        } else {
            &line_string
        };

        let captures = ROLE_TARGET_PATTERN.captures(text_before_cursor)?;

        let role_match = captures.name("role")?;
        let target_match = captures.name("target")?;

        let role_type = match role_match.as_str() {
            "ref" => RoleType::Ref,
            "numref" => RoleType::NumRef,
            "doc" => RoleType::Doc,
            "download" => RoleType::Download,
            "term" => RoleType::Term,
            _ => return None,
        };

        // Calculate where the target starts (position after the backtick)
        let target_start = target_match.start() as u32;

        Some(Self {
            role_type,
            partial_target: target_match.as_str().to_string(),
            line: line as u32,
            target_start,
            character: character as u32,
            vault: context.vault,
            path: context.path,
        })
    }

    fn completions(&self) -> Vec<impl Completable<'a, Self>>
    where
        Self: Sized,
    {
        let referenceables = self.vault.select_referenceable_nodes(None);
        let partial_lower = self.partial_target.to_lowercase();

        referenceables
            .into_iter()
            .filter_map(|referenceable| {
                let completion = RoleCompletion::from_referenceable(
                    referenceable,
                    self.role_type,
                    self.vault,
                    self.path,
                )?;

                // Filter by partial input
                if partial_lower.is_empty()
                    || completion.label.to_lowercase().contains(&partial_lower)
                {
                    Some(completion)
                } else {
                    None
                }
            })
            .collect()
    }

    type FilterParams = ();

    fn completion_filter_text(&self, _params: Self::FilterParams) -> String {
        // Output: {role}`partial where role is inserted via format
        // {{ escapes to literal {, }} escapes to literal }
        // So: {{ + {} + }} + ` + {} = {role}`partial
        format!("{{{}}}`{}", self.role_type.name(), self.partial_target)
    }
}

/// A completion item for a MyST role target.
#[derive(Debug, Clone)]
pub struct RoleCompletion {
    /// The label to display (anchor name, heading text, or file path)
    pub label: String,
    /// The text to insert as the role target
    pub insert_text: String,
    /// Optional detail (e.g., file path where target is defined)
    pub detail: Option<String>,
    /// The kind of completion item
    pub kind: CompletionItemKind,
}

impl RoleCompletion {
    /// Create a role completion from a referenceable, if applicable for the role type.
    fn from_referenceable(
        referenceable: Referenceable<'_>,
        role_type: RoleType,
        vault: &Vault,
        current_path: &std::path::Path,
    ) -> Option<Self> {
        match (role_type, &referenceable) {
            // {ref} and {numref} complete anchors
            (RoleType::Ref | RoleType::NumRef, Referenceable::MystAnchor(path, symbol)) => {
                let detail = get_relative_ref_path(vault.root_dir(), path);
                Some(RoleCompletion {
                    label: symbol.name.clone(),
                    insert_text: symbol.name.clone(),
                    detail,
                    kind: CompletionItemKind::REFERENCE,
                })
            }
            // {ref} and {numref} also complete headings
            (RoleType::Ref | RoleType::NumRef, Referenceable::Heading(path, heading)) => {
                let detail = get_relative_ref_path(vault.root_dir(), path);
                // MyST uses heading text slugified as target
                let slug = slugify_heading(&heading.heading_text);
                Some(RoleCompletion {
                    label: heading.heading_text.clone(),
                    insert_text: slug,
                    detail,
                    kind: CompletionItemKind::REFERENCE,
                })
            }
            // {doc} completes file paths
            (RoleType::Doc | RoleType::Download, Referenceable::File(path, _mdfile)) => {
                let ref_path = get_relative_ref_path(vault.root_dir(), path)?;
                // Calculate relative path from current file
                let relative_path = calculate_relative_path(current_path, path, vault.root_dir());
                Some(RoleCompletion {
                    label: relative_path.clone(),
                    insert_text: relative_path,
                    detail: Some(ref_path),
                    kind: CompletionItemKind::FILE,
                })
            }
            // {term} completes glossary terms
            (RoleType::Term, Referenceable::GlossaryTerm(path, term)) => {
                let detail = get_relative_ref_path(vault.root_dir(), path);
                Some(RoleCompletion {
                    label: term.term.clone(),
                    insert_text: term.term.clone(),
                    detail,
                    kind: CompletionItemKind::TEXT,
                })
            }
            _ => None,
        }
    }
}

impl<'a> Completable<'a, MystRoleCompleter<'a>> for RoleCompletion {
    fn completions(&self, completer: &MystRoleCompleter<'a>) -> Option<CompletionItem> {
        let filter_text = completer.completion_filter_text(());

        Some(CompletionItem {
            label: self.label.clone(),
            detail: self.detail.clone(),
            kind: Some(self.kind),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: Range {
                    start: Position {
                        line: completer.line,
                        character: completer.target_start,
                    },
                    end: Position {
                        line: completer.line,
                        character: completer.character,
                    },
                },
                new_text: self.insert_text.clone(),
            })),
            filter_text: Some(filter_text),
            ..Default::default()
        })
    }
}

/// Slugify a heading text for use as a MyST reference target.
/// This converts "My Heading Text" to "my-heading-text".
fn slugify_heading(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Calculate a relative path from current_path to target_path.
fn calculate_relative_path(
    current_path: &std::path::Path,
    target_path: &std::path::Path,
    root_dir: &std::path::Path,
) -> String {
    use pathdiff::diff_paths;

    // Get the directory containing the current file
    let current_dir = current_path.parent().unwrap_or(root_dir);

    // Calculate relative path from current directory to target
    if let Some(relative) = diff_paths(target_path, current_dir) {
        let path_str = relative.with_extension("").display().to_string();
        // Ensure it starts with ./ for clarity
        if !path_str.starts_with("..") && !path_str.starts_with('/') {
            format!("./{}", path_str)
        } else {
            path_str
        }
    } else {
        // Fallback to absolute-like path from root
        get_relative_ref_path(root_dir, target_path).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Test helpers
    // =========================================================================

    /// Parse a role pattern from text, returning (role_type, partial_target).
    ///
    /// This helper mirrors the regex logic in `MystRoleCompleter::construct`,
    /// enabling unit tests to verify pattern matching without constructing
    /// a full vault context.
    fn parse_role_pattern(text: &str, cursor_pos: usize) -> Option<(RoleType, String)> {
        // Pattern includes all supported role types (ref, numref, doc, download, term)
        static ROLE_TARGET_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\{(?<role>ref|numref|doc|download|term)\}`(?<target>[^`]*)$").unwrap()
        });

        let text_before_cursor = if cursor_pos <= text.len() {
            &text[..cursor_pos]
        } else {
            text
        };

        let captures = ROLE_TARGET_PATTERN.captures(text_before_cursor)?;

        let role_match = captures.name("role")?;
        let target_match = captures.name("target")?;

        let role_type = match role_match.as_str() {
            "ref" => RoleType::Ref,
            "numref" => RoleType::NumRef,
            "doc" => RoleType::Doc,
            "download" => RoleType::Download,
            "term" => RoleType::Term,
            _ => return None,
        };

        Some((role_type, target_match.as_str().to_string()))
    }

    // =========================================================================
    // RED PHASE: Tests for role pattern detection
    // =========================================================================

    mod pattern_detection {
        use super::*;

        #[test]
        fn test_ref_role_with_empty_target() {
            // Test: {ref}`
            let input = "{ref}`";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{ref}}` pattern");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Ref);
            assert_eq!(partial, "");
        }

        #[test]
        fn test_ref_role_with_partial_target() {
            // Test: {ref}`my-
            let input = "{ref}`my-";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{ref}}`partial pattern");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Ref);
            assert_eq!(partial, "my-");
        }

        #[test]
        fn test_numref_role_with_partial_target() {
            // Test: {numref}`fig-
            let input = "{numref}`fig-";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{numref}}` pattern");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::NumRef);
            assert_eq!(partial, "fig-");
        }

        #[test]
        fn test_doc_role_with_empty_target() {
            // Test: {doc}`
            let input = "{doc}`";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{doc}}` pattern");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Doc);
            assert_eq!(partial, "");
        }

        #[test]
        fn test_doc_role_with_path() {
            // Test: {doc}`./path/to
            let input = "{doc}`./path/to";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{doc}}` with path");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Doc);
            assert_eq!(partial, "./path/to");
        }

        #[test]
        fn test_download_role() {
            // Test: {download}`file.
            let input = "{download}`file.";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{download}}` pattern");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Download);
            assert_eq!(partial, "file.");
        }

        #[test]
        fn test_role_in_middle_of_line() {
            // Test: See {ref}`my-
            let input = "See {ref}`my-";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match role in middle of line");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Ref);
            assert_eq!(partial, "my-");
        }

        #[test]
        fn test_role_after_other_content() {
            // Test: For details, see {doc}`./api/
            let input = "For details, see {doc}`./api/";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match role after text");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Doc);
            assert_eq!(partial, "./api/");
        }
    }

    mod pattern_rejection {
        use super::*;

        #[test]
        fn test_reject_plain_text() {
            let input = "This is plain text";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_none(), "Should not match plain text");
        }

        #[test]
        fn test_reject_incomplete_role() {
            // Test: {ref} (no backtick)
            let input = "{ref}";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_none(), "Should not match role without backtick");
        }

        #[test]
        fn test_reject_closed_role() {
            // Test: {ref}`target` (already closed)
            let input = "{ref}`target`";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_none(), "Should not match closed role");
        }

        #[test]
        fn test_reject_unknown_role() {
            // Test: {unknown}`target
            let input = "{unknown}`target";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_none(), "Should not match unknown role");
        }

        #[test]
        fn test_reject_directive_pattern() {
            // Test: ```{note}
            let input = "```{note}";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_none(), "Should not match directive pattern");
        }

        #[test]
        fn test_reject_markdown_link() {
            // Test: [link](url)
            let input = "[link](url)";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_none(), "Should not match markdown link");
        }

        #[test]
        fn test_reject_role_with_display_text() {
            // Test: {ref}`Display Text <target>` - user is inside display, not target
            // This is tricky - for now we just complete the whole thing
            let input = "{ref}`Display Text <";
            let result = parse_role_pattern(input, input.len());
            // This should still match since we're after the opening backtick
            assert!(
                result.is_some(),
                "Should match even with display text syntax"
            );
        }
    }

    // =========================================================================
    // Tests for RoleType methods
    // =========================================================================

    mod role_type_tests {
        use super::*;

        #[test]
        fn test_role_names() {
            assert_eq!(RoleType::Ref.name(), "ref");
            assert_eq!(RoleType::NumRef.name(), "numref");
            assert_eq!(RoleType::Doc.name(), "doc");
            assert_eq!(RoleType::Download.name(), "download");
        }
    }

    // =========================================================================
    // Tests for slugify_heading
    // =========================================================================

    mod slugify_tests {
        use super::*;

        #[test]
        fn test_simple_heading() {
            assert_eq!(slugify_heading("My Heading"), "my-heading");
        }

        #[test]
        fn test_heading_with_special_chars() {
            assert_eq!(slugify_heading("Hello, World!"), "hello-world");
        }

        #[test]
        fn test_heading_with_numbers() {
            assert_eq!(
                slugify_heading("Chapter 1: Introduction"),
                "chapter-1-introduction"
            );
        }

        #[test]
        fn test_heading_already_slugified() {
            assert_eq!(slugify_heading("my-heading"), "my-heading");
        }

        #[test]
        fn test_heading_with_multiple_spaces() {
            assert_eq!(slugify_heading("My   Heading"), "my-heading");
        }
    }

    // =========================================================================
    // Tests for {term} role pattern detection
    // =========================================================================

    mod term_role_tests {
        use super::*;

        #[test]
        fn test_term_role_with_empty_target() {
            let input = "{term}`";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{term}}` pattern");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Term);
            assert_eq!(partial, "");
        }

        #[test]
        fn test_term_role_with_partial_target() {
            let input = "{term}`My";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{term}}`partial pattern");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Term);
            assert_eq!(partial, "My");
        }

        #[test]
        fn test_term_role_in_sentence() {
            let input = "The {term}`dialectical";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{term}}` in sentence");
            let (role_type, partial) = result.unwrap();
            assert_eq!(role_type, RoleType::Term);
            assert_eq!(partial, "dialectical");
        }

        #[test]
        fn test_term_role_name() {
            assert_eq!(RoleType::Term.name(), "term");
        }
    }
}
