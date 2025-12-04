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
    /// {eq}`label` - equation reference
    Eq,
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
            RoleType::Eq => "eq",
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
        // where role is one of: ref, numref, doc, download, term, eq
        // We match up to the cursor position, target may be incomplete
        static ROLE_TARGET_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\{(?<role>ref|numref|doc|download|term|eq)\}`(?<target>[^`]*)$").unwrap()
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
            "eq" => RoleType::Eq,
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

        // Check if we're using root-relative path syntax (starts with /)
        let uses_root_prefix = self.partial_target.starts_with('/');

        referenceables
            .into_iter()
            .filter_map(|referenceable| {
                // For Doc/Download roles, use the root-prefix-aware method
                let completion = if matches!(self.role_type, RoleType::Doc | RoleType::Download) {
                    RoleCompletion::from_referenceable_with_root_prefix(
                        referenceable,
                        self.role_type,
                        self.vault,
                        self.path,
                        &self.partial_target,
                    )
                } else {
                    RoleCompletion::from_referenceable(
                        referenceable,
                        self.role_type,
                        self.vault,
                        self.path,
                    )
                }?;

                // Filter by partial input (root-prefix method handles its own filtering)
                if uses_root_prefix {
                    // Already filtered by from_referenceable_with_root_prefix
                    Some(completion)
                } else if partial_lower.is_empty()
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
            // {eq} completes math equation labels
            (RoleType::Eq, Referenceable::MathLabel(path, symbol)) => {
                let label = symbol.label.as_ref()?;
                let detail = get_relative_ref_path(vault.root_dir(), path);
                Some(RoleCompletion {
                    label: label.clone(),
                    insert_text: label.clone(),
                    detail,
                    kind: CompletionItemKind::REFERENCE,
                })
            }
            _ => None,
        }
    }

    /// Create a role completion with support for root-relative paths.
    ///
    /// When `partial_target` starts with `/`, paths are calculated from the vault root
    /// instead of relative to the current file. The `/` prefix is preserved in the
    /// insert text.
    ///
    /// # Arguments
    ///
    /// * `referenceable` - The referenceable item to create a completion for
    /// * `role_type` - The type of role being completed
    /// * `vault` - Reference to the vault
    /// * `current_path` - Path of the current file
    /// * `partial_target` - The partial target typed so far (may start with `/`)
    ///
    /// # Returns
    ///
    /// `Some(RoleCompletion)` if the referenceable matches the role type and
    /// passes the filter, `None` otherwise.
    fn from_referenceable_with_root_prefix(
        referenceable: Referenceable<'_>,
        role_type: RoleType,
        vault: &Vault,
        current_path: &std::path::Path,
        partial_target: &str,
    ) -> Option<Self> {
        let use_root_prefix = partial_target.starts_with('/');

        match (role_type, &referenceable) {
            // {doc} and {download} complete file paths
            (RoleType::Doc | RoleType::Download, Referenceable::File(target_path, _mdfile)) => {
                let ref_path = get_relative_ref_path(vault.root_dir(), target_path)?;

                let insert_text = if use_root_prefix {
                    // Calculate path from vault root, prefixed with /
                    calculate_root_relative_path(target_path, vault.root_dir())
                } else {
                    // Use standard relative path from current file
                    calculate_relative_path(current_path, target_path, vault.root_dir())
                };

                // Filter: if partial_target has content, check if insert_text matches
                if !partial_target.is_empty() {
                    let filter_target = if use_root_prefix {
                        &partial_target[1..] // Remove leading /
                    } else {
                        partial_target
                    };

                    // Check if the path matches the filter
                    if !filter_target.is_empty() && !insert_text.contains(filter_target) {
                        return None;
                    }
                }

                Some(RoleCompletion {
                    label: insert_text.clone(),
                    insert_text,
                    detail: Some(ref_path),
                    kind: CompletionItemKind::FILE,
                })
            }
            // For non-file completions, delegate to the standard method
            _ => Self::from_referenceable(referenceable, role_type, vault, current_path),
        }
    }
}

/// Calculate a path from the vault root, prefixed with `/`.
///
/// This is used when the user types `/` at the start of a target to indicate
/// they want an absolute path from the vault root.
fn calculate_root_relative_path(
    target_path: &std::path::Path,
    root_dir: &std::path::Path,
) -> String {
    // Strip the root_dir prefix and the .md extension
    if let Ok(relative) = target_path.strip_prefix(root_dir) {
        let path_str = relative.with_extension("").display().to_string();
        format!("/{}", path_str)
    } else {
        // Fallback: just use the filename
        let filename = target_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        format!("/{}", filename)
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
        // Pattern includes all supported role types (ref, numref, doc, download, term, eq)
        static ROLE_TARGET_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\{(?<role>ref|numref|doc|download|term|eq)\}`(?<target>[^`]*)$").unwrap()
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
            "eq" => RoleType::Eq,
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

        #[test]
        fn test_eq_role_with_empty_target() {
            let input = "{eq}`";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{eq}}` pattern");
            let (role, partial) = result.unwrap();
            assert_eq!(role, RoleType::Eq);
            assert_eq!(partial, "");
        }

        #[test]
        fn test_eq_role_with_partial_target() {
            let input = "{eq}`euler";
            let result = parse_role_pattern(input, input.len());
            assert!(result.is_some(), "Should match {{eq}}`partial pattern");
            let (role, partial) = result.unwrap();
            assert_eq!(role, RoleType::Eq);
            assert_eq!(partial, "euler");
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

        #[test]
        fn test_eq_role_name() {
            assert_eq!(RoleType::Eq.name(), "eq");
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

    // =========================================================================
    // {eq} Role Completion Tests
    // =========================================================================
    //
    // Tests for {eq} role completion integration with MathLabel referenceables.
    // The {eq} role should show completions from `{math}` directive `:label:` values.

    mod eq_role_completions {
        use super::*;
        use crate::config::Settings;
        use crate::test_utils::create_test_vault_dir;
        use crate::vault::{Referenceable, Vault};
        use std::fs;

        /// Test: {eq} role completions include MathLabel referenceables.
        ///
        /// When a vault contains math directives with labels, the {eq} role
        /// completer should return those labels as completion options.
        #[test]
        fn test_eq_role_completions_show_math_labels() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with math directives containing labels
            fs::write(
                vault_dir.join("equations.md"),
                r#"# Equations

```{math}
:label: euler-identity

e^{i\pi} + 1 = 0
```

```{math}
:label: pythagorean

a^2 + b^2 = c^2
```
"#,
            )
            .unwrap();

            let settings = Settings::default();
            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            // Get all referenceables from the vault
            let referenceables = vault.select_referenceable_nodes(None);

            // Find MathLabel referenceables
            let math_labels: Vec<_> = referenceables
                .iter()
                .filter(|r| matches!(r, Referenceable::MathLabel(..)))
                .collect();

            assert!(
                !math_labels.is_empty(),
                "Should have MathLabel referenceables in vault"
            );

            // Create RoleCompletions for Eq role from MathLabel referenceables
            let completions: Vec<_> = math_labels
                .iter()
                .filter_map(|r| {
                    RoleCompletion::from_referenceable(
                        (*r).clone(),
                        RoleType::Eq,
                        &vault,
                        &vault_dir.join("test.md"),
                    )
                })
                .collect();

            // Should have completions for math labels
            assert_eq!(
                completions.len(),
                2,
                "Should have 2 completions for 2 math labels"
            );

            // Verify completion labels
            let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
            assert!(
                labels.contains(&"euler-identity"),
                "Should contain euler-identity: {:?}",
                labels
            );
            assert!(
                labels.contains(&"pythagorean"),
                "Should contain pythagorean: {:?}",
                labels
            );
        }

        /// Test: {eq} role completions only show MathLabels, not other referenceables.
        ///
        /// The {eq} role should filter out anchors, headings, files, and glossary terms.
        #[test]
        fn test_eq_role_filters_non_math_referenceables() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with various referenceables (anchors, headings, glossary, math)
            fs::write(
                vault_dir.join("mixed.md"),
                r#"# Mixed Content

(my-anchor)=
## Section with Anchor

```{math}
:label: only-equation

x = y
```

```{glossary}
API
  Application Programming Interface.
```
"#,
            )
            .unwrap();

            let settings = Settings::default();
            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            // Get all referenceables
            let referenceables = vault.select_referenceable_nodes(None);

            // Try to create completions for ALL referenceables with Eq role
            let eq_completions: Vec<_> = referenceables
                .iter()
                .filter_map(|r| {
                    RoleCompletion::from_referenceable(
                        (*r).clone(),
                        RoleType::Eq,
                        &vault,
                        &vault_dir.join("test.md"),
                    )
                })
                .collect();

            // Should only have 1 completion (the math label)
            assert_eq!(
                eq_completions.len(),
                1,
                "{{eq}} role should only show MathLabel completions. Got: {:?}",
                eq_completions
            );

            assert_eq!(
                eq_completions[0].label, "only-equation",
                "Should only show the math equation label"
            );
        }

        /// Test: {eq} completion kind is appropriate.
        ///
        /// Math label completions should have an appropriate completion kind.
        #[test]
        fn test_eq_completion_item_kind() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            fs::write(
                vault_dir.join("equations.md"),
                r#"```{math}
:label: test-equation

x = 1
```
"#,
            )
            .unwrap();

            let settings = Settings::default();
            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let referenceables = vault.select_referenceable_nodes(None);
            let math_label = referenceables
                .iter()
                .find(|r| matches!(r, Referenceable::MathLabel(..)))
                .expect("Should find MathLabel");

            let completion = RoleCompletion::from_referenceable(
                math_label.clone(),
                RoleType::Eq,
                &vault,
                &vault_dir.join("test.md"),
            );

            assert!(completion.is_some(), "Should create completion");
            let completion = completion.unwrap();

            // Math labels should use REFERENCE kind (like anchors)
            assert_eq!(
                completion.kind,
                tower_lsp::lsp_types::CompletionItemKind::REFERENCE,
                "Math label completion should use REFERENCE kind"
            );
        }
    }

    // =========================================================================
    // Root-Relative Path Completion Tests
    // =========================================================================
    //
    // Tests for the `/` prefix path completion feature.
    // When a user types `/` at the start of a target, paths should be
    // calculated from the vault root instead of relative to the current file.

    mod root_relative_path_tests {
        use super::*;
        use crate::config::Settings;
        use crate::test_utils::create_test_vault_dir;
        use crate::vault::Vault;
        use std::fs;

        /// Test: {doc}`/ prefix suggests files from vault root.
        ///
        /// When typing `{doc}`/`, completions should show files relative
        /// to the vault root, not the current file's directory.
        #[test]
        fn test_root_prefix_suggests_from_vault_root() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create directory structure
            fs::create_dir(vault_dir.join("guides")).unwrap();
            fs::create_dir(vault_dir.join("api")).unwrap();

            // Files at different levels
            fs::write(vault_dir.join("index.md"), "# Index").unwrap();
            fs::write(vault_dir.join("guides/intro.md"), "# Intro").unwrap();
            fs::write(vault_dir.join("api/reference.md"), "# Reference").unwrap();

            let settings = Settings::default();
            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            // Current file is in a subdirectory
            let current_path = vault_dir.join("guides/intro.md");

            // Get completions for {doc}` with root prefix /
            let referenceables = vault.select_referenceable_nodes(None);
            let partial_target = "/";

            let completions: Vec<_> = referenceables
                .iter()
                .filter_map(|r| {
                    RoleCompletion::from_referenceable_with_root_prefix(
                        r.clone(),
                        RoleType::Doc,
                        &vault,
                        &current_path,
                        partial_target,
                    )
                })
                .collect();

            // Should have completions for files
            assert!(
                !completions.is_empty(),
                "Should have completions with root prefix"
            );

            // Completions should use root-relative paths (starting with /)
            let labels: Vec<_> = completions.iter().map(|c| c.insert_text.as_str()).collect();
            assert!(
                labels.iter().any(|l| l.starts_with("/")),
                "At least one completion should start with /. Got: {:?}",
                labels
            );
        }

        /// Test: Root prefix is preserved in insert text.
        ///
        /// When selecting a completion from a root-prefixed search,
        /// the insert text should preserve the `/` prefix.
        #[test]
        fn test_root_prefix_preserved_in_insert_text() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            fs::create_dir(vault_dir.join("guides")).unwrap();
            fs::write(vault_dir.join("guides/intro.md"), "# Intro").unwrap();

            let settings = Settings::default();
            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            // Current file in root
            let current_path = vault_dir.join("index.md");

            let referenceables = vault.select_referenceable_nodes(None);
            let partial_target = "/guides/";

            let completions: Vec<_> = referenceables
                .iter()
                .filter_map(|r| {
                    RoleCompletion::from_referenceable_with_root_prefix(
                        r.clone(),
                        RoleType::Doc,
                        &vault,
                        &current_path,
                        partial_target,
                    )
                })
                .collect();

            // Find intro.md completion
            let intro_completion = completions
                .iter()
                .find(|c| c.label.contains("intro") || c.insert_text.contains("intro"));

            assert!(
                intro_completion.is_some(),
                "Should find intro completion. Got: {:?}",
                completions
            );

            let intro = intro_completion.unwrap();
            assert!(
                intro.insert_text.starts_with("/"),
                "Insert text should start with /: {}",
                intro.insert_text
            );
            assert!(
                intro.insert_text.contains("guides"),
                "Insert text should contain path: {}",
                intro.insert_text
            );
        }

        /// Test: Relative path still works (no regression).
        ///
        /// When NOT using the `/` prefix, paths should still be relative
        /// to the current file.
        #[test]
        fn test_relative_path_still_works() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            fs::create_dir(vault_dir.join("guides")).unwrap();
            fs::write(vault_dir.join("guides/intro.md"), "# Intro").unwrap();
            fs::write(vault_dir.join("guides/advanced.md"), "# Advanced").unwrap();

            let settings = Settings::default();
            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            // Current file is in guides/
            let current_path = vault_dir.join("guides/intro.md");

            let referenceables = vault.select_referenceable_nodes(None);

            // Without root prefix, use regular relative path completion
            let completions: Vec<_> = referenceables
                .iter()
                .filter_map(|r| {
                    RoleCompletion::from_referenceable(
                        r.clone(),
                        RoleType::Doc,
                        &vault,
                        &current_path,
                    )
                })
                .collect();

            // Find advanced.md completion
            let advanced_completion = completions.iter().find(|c| c.label.contains("advanced"));

            assert!(
                advanced_completion.is_some(),
                "Should find advanced completion"
            );

            let advanced = advanced_completion.unwrap();
            // Relative path should use ./ or bare name, not /
            assert!(
                !advanced.insert_text.starts_with("/"),
                "Relative path should not start with /: {}",
                advanced.insert_text
            );
        }

        /// Test: Root prefix with subdirectory filters correctly.
        ///
        /// When typing `{doc}`/guides/`, only files in the guides directory
        /// should be shown.
        #[test]
        fn test_root_with_subdirectory() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            fs::create_dir(vault_dir.join("guides")).unwrap();
            fs::create_dir(vault_dir.join("api")).unwrap();
            fs::write(vault_dir.join("guides/intro.md"), "# Intro").unwrap();
            fs::write(vault_dir.join("api/reference.md"), "# Reference").unwrap();

            let settings = Settings::default();
            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let current_path = vault_dir.join("index.md");
            let referenceables = vault.select_referenceable_nodes(None);
            let partial_target = "/guides/";

            let completions: Vec<_> = referenceables
                .iter()
                .filter_map(|r| {
                    RoleCompletion::from_referenceable_with_root_prefix(
                        r.clone(),
                        RoleType::Doc,
                        &vault,
                        &current_path,
                        partial_target,
                    )
                })
                .collect();

            // Should only show files in guides/, not api/
            let labels: Vec<_> = completions.iter().map(|c| c.insert_text.as_str()).collect();
            assert!(
                labels.iter().any(|l| l.contains("guides")),
                "Should have guides completions: {:?}",
                labels
            );
            assert!(
                !labels.iter().any(|l| l.contains("api")),
                "Should NOT have api completions when filtering by /guides/: {:?}",
                labels
            );
        }
    }
}
