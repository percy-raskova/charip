//! Role Name Completer
//!
//! Provides autocomplete for MyST role names when user types `{`.
//!
//! ## Trigger Pattern
//! - `{` - triggers suggestion of all role names
//! - `{do` - filters to roles starting with "do" (doc, download)
//!
//! ## Completion Behavior
//! Selecting a role (e.g., "doc") inserts the complete pattern with snippet:
//! `{doc}`$1`` - cursor positioned between backticks for target entry.
//!
//! ## Role Types
//! - ref, numref - cross-references to anchors/headings
//! - doc, download - document/file references
//! - term - glossary term references
//! - eq - equation label references

use once_cell::sync::Lazy;
use regex::Regex;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, InsertTextFormat, Position, Range,
    TextEdit,
};

use super::util::check_in_code_block;
use super::{Completable, Completer, Context};

/// The available MyST roles for completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MystRole {
    /// {ref}`target` - cross-reference to anchor or heading
    Ref,
    /// {numref}`target` - numbered reference
    NumRef,
    /// {doc}`path` - document reference
    Doc,
    /// {download}`path` - downloadable file reference
    Download,
    /// {term}`glossary-term` - glossary term reference
    Term,
    /// {eq}`label` - equation reference
    Eq,
}

impl MystRole {
    /// Returns all available MyST roles.
    pub fn all() -> Vec<Self> {
        vec![
            Self::Ref,
            Self::NumRef,
            Self::Doc,
            Self::Download,
            Self::Term,
            Self::Eq,
        ]
    }

    /// Returns the role name as it appears in MyST syntax.
    pub fn name(&self) -> &'static str {
        match self {
            MystRole::Ref => "ref",
            MystRole::NumRef => "numref",
            MystRole::Doc => "doc",
            MystRole::Download => "download",
            MystRole::Term => "term",
            MystRole::Eq => "eq",
        }
    }

    /// Returns a description for the role.
    pub fn description(&self) -> &'static str {
        match self {
            MystRole::Ref => "Cross-reference to anchor or heading",
            MystRole::NumRef => "Numbered reference to anchor or heading",
            MystRole::Doc => "Reference to another document",
            MystRole::Download => "Link to downloadable file",
            MystRole::Term => "Reference to glossary term",
            MystRole::Eq => "Reference to equation label",
        }
    }

    /// Returns the LSP snippet format for this role.
    /// Format: {role}`$1` with cursor positioned between backticks.
    pub fn snippet(&self) -> String {
        format!("{{{}}}\\`$1\\`", self.name())
    }
}

/// Completer for MyST role names.
///
/// Activates when the cursor is positioned after `{` at the start of a role,
/// for example: `{|` or `{do|` where `|` is the cursor.
pub struct RoleNameCompleter {
    /// The partial role name typed so far (may be empty)
    partial_role: String,
    /// Line number in the document
    line: u32,
    /// Character position where the `{` starts
    brace_start: u32,
    /// Current cursor position
    character: u32,
}

impl<'a> Completer<'a> for RoleNameCompleter {
    fn construct(context: Context<'a>, line: usize, character: usize) -> Option<Self>
    where
        Self: Sized + Completer<'a>,
    {
        // Don't trigger in code blocks
        if check_in_code_block(&context, line, character) {
            return None;
        }

        let line_chars = context.vault.select_line(context.path, line as isize)?;
        let line_string = String::from_iter(line_chars);

        // Pattern: `{` followed by optional partial role name
        // Must NOT be:
        // - After ``` or ::: (directive context)
        // - Already closed with `}`
        // - Inside a completed role `{role}`
        static ROLE_NAME_PATTERN: Lazy<Regex> = Lazy::new(|| {
            // Match { followed by optional partial name, but not after fence markers
            // Negative lookbehind simulation: check that we're not after ``` or :::
            Regex::new(r"\{(?<partial>[a-z]*)$").unwrap()
        });

        // Get the text up to the cursor position
        let text_before_cursor = if character <= line_string.len() {
            &line_string[..character]
        } else {
            &line_string
        };

        // Reject if this looks like a directive context (after fence markers)
        // Check for ``` or ::: before the {
        static DIRECTIVE_CONTEXT: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(`{3,}|:{3,})\{[a-z]*$").unwrap());

        if DIRECTIVE_CONTEXT.is_match(text_before_cursor) {
            return None;
        }

        let captures = ROLE_NAME_PATTERN.captures(text_before_cursor)?;
        let partial_match = captures.name("partial")?;

        // Calculate where the { starts
        let brace_start = (text_before_cursor.len() - partial_match.as_str().len() - 1) as u32;

        Some(Self {
            partial_role: partial_match.as_str().to_string(),
            line: line as u32,
            brace_start,
            character: character as u32,
        })
    }

    fn completions(&self) -> Vec<impl Completable<'a, Self>>
    where
        Self: Sized,
    {
        let partial_lower = self.partial_role.to_lowercase();

        MystRole::all()
            .into_iter()
            .filter(|role| {
                if partial_lower.is_empty() {
                    true
                } else {
                    role.name().starts_with(&partial_lower)
                }
            })
            .collect()
    }

    type FilterParams = &'static str;

    fn completion_filter_text(&self, params: Self::FilterParams) -> String {
        format!("{{{}", params)
    }
}

impl<'a> Completable<'a, RoleNameCompleter> for MystRole {
    fn completions(&self, completer: &RoleNameCompleter) -> Option<CompletionItem> {
        let filter_text = completer.completion_filter_text(self.name());

        Some(CompletionItem {
            label: self.name().to_string(),
            detail: Some(self.description().to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            kind: Some(CompletionItemKind::KEYWORD),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: Range {
                    start: Position {
                        line: completer.line,
                        character: completer.brace_start,
                    },
                    end: Position {
                        line: completer.line,
                        character: completer.character,
                    },
                },
                new_text: self.snippet(),
            })),
            filter_text: Some(filter_text),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Test helpers
    // =========================================================================

    /// Parse a role name pattern from text, returning partial role name if matched.
    ///
    /// This helper mirrors the regex logic in `RoleNameCompleter::construct`,
    /// enabling unit tests to verify pattern matching without a vault context.
    fn parse_role_name_pattern(text: &str, cursor_pos: usize) -> Option<String> {
        static ROLE_NAME_PATTERN: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"\{(?<partial>[a-z]*)$").unwrap());

        static DIRECTIVE_CONTEXT: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(`{3,}|:{3,})\{[a-z]*$").unwrap());

        let text_before_cursor = if cursor_pos <= text.len() {
            &text[..cursor_pos]
        } else {
            text
        };

        // Reject directive context
        if DIRECTIVE_CONTEXT.is_match(text_before_cursor) {
            return None;
        }

        let captures = ROLE_NAME_PATTERN.captures(text_before_cursor)?;
        let partial_match = captures.name("partial")?;

        Some(partial_match.as_str().to_string())
    }

    // =========================================================================
    // RED PHASE: Tests for pattern detection
    // =========================================================================

    mod pattern_detection {
        use super::*;

        #[test]
        fn test_matches_opening_brace() {
            // Test: {
            let input = "{";
            let result = parse_role_name_pattern(input, input.len());
            assert!(result.is_some(), "Should match opening brace");
            assert_eq!(result.unwrap(), "", "Partial should be empty");
        }

        #[test]
        fn test_matches_partial_role_name_do() {
            // Test: {do -> should suggest doc, download
            let input = "{do";
            let result = parse_role_name_pattern(input, input.len());
            assert!(result.is_some(), "Should match partial role name");
            assert_eq!(result.unwrap(), "do", "Partial should be 'do'");
        }

        #[test]
        fn test_matches_partial_role_name_ref() {
            // Test: {re -> should suggest ref
            let input = "{re";
            let result = parse_role_name_pattern(input, input.len());
            assert!(result.is_some(), "Should match partial role name");
            assert_eq!(result.unwrap(), "re", "Partial should be 're'");
        }

        #[test]
        fn test_matches_in_sentence() {
            // Test: See {
            let input = "See {";
            let result = parse_role_name_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{ in sentence");
            assert_eq!(result.unwrap(), "");
        }

        #[test]
        fn test_matches_after_text_with_partial() {
            // Test: See {do
            let input = "See {do";
            let result = parse_role_name_pattern(input, input.len());
            assert!(result.is_some(), "Should match partial role after text");
            assert_eq!(result.unwrap(), "do");
        }
    }

    mod pattern_rejection {
        use super::*;

        #[test]
        fn test_does_not_match_closed_role() {
            // Test: {doc} - already closed, should not trigger
            let input = "{doc}";
            let result = parse_role_name_pattern(input, input.len());
            assert!(
                result.is_none(),
                "Should not match closed role (ends with }})"
            );
        }

        #[test]
        fn test_does_not_match_complete_role_with_backtick() {
            // Test: {doc}` - role is complete, MystRoleCompleter handles this
            let input = "{doc}`";
            let result = parse_role_name_pattern(input, input.len());
            assert!(
                result.is_none(),
                "Should not match complete role with backtick"
            );
        }

        #[test]
        fn test_does_not_match_directive_context_backticks() {
            // Test: ```{note - this is a directive, not a role
            let input = "```{note";
            let result = parse_role_name_pattern(input, input.len());
            assert!(result.is_none(), "Should not match directive context");
        }

        #[test]
        fn test_does_not_match_directive_context_colons() {
            // Test: :::{note - this is a directive, not a role
            let input = ":::{note";
            let result = parse_role_name_pattern(input, input.len());
            assert!(result.is_none(), "Should not match colon directive context");
        }

        #[test]
        fn test_does_not_match_longer_fence_directive() {
            // Test: ````{tip
            let input = "````{tip";
            let result = parse_role_name_pattern(input, input.len());
            assert!(
                result.is_none(),
                "Should not match longer fence directive context"
            );
        }

        #[test]
        fn test_does_not_match_plain_text() {
            let input = "This is plain text";
            let result = parse_role_name_pattern(input, input.len());
            assert!(result.is_none(), "Should not match plain text");
        }
    }

    // =========================================================================
    // RED PHASE: Tests for completion output
    // =========================================================================

    mod completion_output {
        use super::*;

        #[test]
        fn test_completion_inserts_snippet_format() {
            // The snippet should be {role}`$1` format for cursor placement
            let snippet = MystRole::Doc.snippet();
            assert!(
                snippet.contains("{doc}"),
                "Snippet should contain role syntax"
            );
            assert!(
                snippet.contains("$1"),
                "Snippet should have cursor position"
            );
            assert!(
                snippet.contains("\\`"),
                "Snippet should have escaped backticks"
            );
        }

        #[test]
        fn test_all_role_types_suggested() {
            let roles = MystRole::all();
            let names: Vec<&str> = roles.iter().map(|r| r.name()).collect();

            assert!(names.contains(&"ref"), "Should suggest ref");
            assert!(names.contains(&"numref"), "Should suggest numref");
            assert!(names.contains(&"doc"), "Should suggest doc");
            assert!(names.contains(&"download"), "Should suggest download");
            assert!(names.contains(&"term"), "Should suggest term");
            assert!(names.contains(&"eq"), "Should suggest eq");
        }

        #[test]
        fn test_role_count() {
            assert_eq!(MystRole::all().len(), 6, "Should have exactly 6 roles");
        }

        #[test]
        fn test_all_roles_have_descriptions() {
            for role in MystRole::all() {
                assert!(
                    !role.description().is_empty(),
                    "Role {:?} should have a description",
                    role
                );
            }
        }

        #[test]
        fn test_snippet_format_ref() {
            let snippet = MystRole::Ref.snippet();
            assert_eq!(
                snippet, "{ref}\\`$1\\`",
                "Ref snippet should have correct format"
            );
        }

        #[test]
        fn test_snippet_format_numref() {
            let snippet = MystRole::NumRef.snippet();
            assert_eq!(
                snippet, "{numref}\\`$1\\`",
                "NumRef snippet should have correct format"
            );
        }

        #[test]
        fn test_snippet_format_doc() {
            let snippet = MystRole::Doc.snippet();
            assert_eq!(
                snippet, "{doc}\\`$1\\`",
                "Doc snippet should have correct format"
            );
        }
    }

    // =========================================================================
    // RED PHASE: Tests for filtering
    // =========================================================================

    mod filtering {
        use super::*;

        #[test]
        fn test_filter_with_do_prefix() {
            let partial = "do";
            let filtered: Vec<_> = MystRole::all()
                .into_iter()
                .filter(|r| r.name().starts_with(partial))
                .collect();

            assert_eq!(filtered.len(), 2, "Should have 2 roles starting with 'do'");
            let names: Vec<_> = filtered.iter().map(|r| r.name()).collect();
            assert!(names.contains(&"doc"));
            assert!(names.contains(&"download"));
        }

        #[test]
        fn test_filter_with_re_prefix() {
            let partial = "re";
            let filtered: Vec<_> = MystRole::all()
                .into_iter()
                .filter(|r| r.name().starts_with(partial))
                .collect();

            assert_eq!(filtered.len(), 1, "Should have 1 role starting with 're'");
            assert_eq!(filtered[0].name(), "ref");
        }

        #[test]
        fn test_filter_with_no_match() {
            let partial = "xyz";
            let filtered: Vec<_> = MystRole::all()
                .into_iter()
                .filter(|r| r.name().starts_with(partial))
                .collect();

            assert_eq!(filtered.len(), 0, "Should have no matches for 'xyz'");
        }

        #[test]
        fn test_filter_empty_returns_all() {
            let partial = "";
            let filtered: Vec<_> = MystRole::all()
                .into_iter()
                .filter(|r| partial.is_empty() || r.name().starts_with(partial))
                .collect();

            assert_eq!(filtered.len(), 6, "Empty filter should return all roles");
        }
    }
}
