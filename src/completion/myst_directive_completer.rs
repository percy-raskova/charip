//! MyST Directive Completer
//!
//! Provides autocomplete for MyST directive names after typing fence markers
//! like ``` or ::: followed by `{`.
//!
//! ## Trigger Patterns
//! - `` ```{ `` or `` ```{partial ``
//! - `:::{` or `:::{partial`
//!
//! The fence can be 3+ backticks or colons.

use once_cell::sync::Lazy;
use regex::Regex;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, InsertTextFormat, Position, Range,
    TextEdit,
};

use super::{Completable, Completer, Context};

/// Completer for MyST directive names.
///
/// Activates when the cursor is positioned after a fence opening with `{`,
/// for example: `` ```{n| `` where `|` is the cursor.
pub struct MystDirectiveCompleter {
    /// The fence marker used (backticks or colons)
    fence_char: char,
    /// Number of fence characters (3 or more)
    fence_count: usize,
    /// The partial directive name typed so far (may be empty)
    partial_directive: String,
    /// Line number in the document
    line: u32,
    /// Character position on the line
    character: u32,
    /// Start position of the fence on the line
    fence_start: usize,
}

impl<'a> Completer<'a> for MystDirectiveCompleter {
    fn construct(context: Context<'a>, line: usize, character: usize) -> Option<Self>
    where
        Self: Sized + Completer<'a>,
    {
        let line_chars = context.vault.select_line(context.path, line as isize)?;
        let line_string = String::from_iter(line_chars);

        // Pattern: 3+ backticks or colons, followed by {, optionally followed by partial name
        // We need to match: ```{, ````{, ````````{, :::{, ::::{, etc.
        static FENCE_DIRECTIVE_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(?<leading>\s*)(?<fence>(`{3,}|:{3,}))\{(?<partial>[a-zA-Z0-9_-]*)$")
                .unwrap()
        });

        // Get the text up to the cursor position
        let text_before_cursor = if character <= line_string.len() {
            &line_string[..character]
        } else {
            &line_string
        };

        let captures = FENCE_DIRECTIVE_PATTERN.captures(text_before_cursor)?;

        let leading = captures.name("leading")?;
        let fence = captures.name("fence")?;
        let partial = captures.name("partial")?;

        let fence_str = fence.as_str();
        let fence_char = fence_str.chars().next()?;
        let fence_count = fence_str.len();

        Some(Self {
            fence_char,
            fence_count,
            partial_directive: partial.as_str().to_string(),
            line: line as u32,
            character: character as u32,
            fence_start: leading.end(),
        })
    }

    fn completions(&self) -> Vec<impl Completable<'a, Self>>
    where
        Self: Sized,
    {
        // Filter directives based on partial input
        let partial_lower = self.partial_directive.to_lowercase();

        MystDirective::all()
            .into_iter()
            .filter(|d| {
                if partial_lower.is_empty() {
                    true
                } else {
                    d.name().to_lowercase().starts_with(&partial_lower)
                }
            })
            .collect()
    }

    type FilterParams = &'static str;

    fn completion_filter_text(&self, params: Self::FilterParams) -> String {
        let fence: String = std::iter::repeat_n(self.fence_char, self.fence_count).collect();
        format!("{}{{{}", fence, params)
    }
}

/// Represents a MyST directive that can be completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MystDirective {
    // Admonitions
    Note,
    Important,
    Hint,
    Tip,
    SeeAlso,
    Attention,
    Caution,
    Warning,
    Danger,
    Error,
    Admonition,

    // Code directives
    CodeBlock,
    LiteralInclude,

    // Media directives
    Figure,
    Image,

    // Table directives
    Table,
    ListTable,
    CsvTable,

    // Structure directives
    Toctree,
    Include,

    // Other
    Math,
    Glossary,
}

impl MystDirective {
    /// Returns all available MyST directives.
    pub fn all() -> Vec<Self> {
        vec![
            // Admonitions
            Self::Note,
            Self::Important,
            Self::Hint,
            Self::Tip,
            Self::SeeAlso,
            Self::Attention,
            Self::Caution,
            Self::Warning,
            Self::Danger,
            Self::Error,
            Self::Admonition,
            // Code
            Self::CodeBlock,
            Self::LiteralInclude,
            // Media
            Self::Figure,
            Self::Image,
            // Tables
            Self::Table,
            Self::ListTable,
            Self::CsvTable,
            // Structure
            Self::Toctree,
            Self::Include,
            // Other
            Self::Math,
            Self::Glossary,
        ]
    }

    /// Returns the directive name as used in MyST syntax.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Important => "important",
            Self::Hint => "hint",
            Self::Tip => "tip",
            Self::SeeAlso => "seealso",
            Self::Attention => "attention",
            Self::Caution => "caution",
            Self::Warning => "warning",
            Self::Danger => "danger",
            Self::Error => "error",
            Self::Admonition => "admonition",
            Self::CodeBlock => "code-block",
            Self::LiteralInclude => "literalinclude",
            Self::Figure => "figure",
            Self::Image => "image",
            Self::Table => "table",
            Self::ListTable => "list-table",
            Self::CsvTable => "csv-table",
            Self::Toctree => "toctree",
            Self::Include => "include",
            Self::Math => "math",
            Self::Glossary => "glossary",
        }
    }

    /// Returns a description for the directive.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Note => "A note admonition",
            Self::Important => "An important notice",
            Self::Hint => "A hint admonition",
            Self::Tip => "A tip admonition",
            Self::SeeAlso => "A see-also reference",
            Self::Attention => "An attention notice",
            Self::Caution => "A caution notice",
            Self::Warning => "A warning admonition",
            Self::Danger => "A danger admonition",
            Self::Error => "An error admonition",
            Self::Admonition => "A custom admonition with title",
            Self::CodeBlock => "A syntax-highlighted code block",
            Self::LiteralInclude => "Include a file as a code block",
            Self::Figure => "A figure with caption",
            Self::Image => "An inline image",
            Self::Table => "A table directive",
            Self::ListTable => "A table from a list structure",
            Self::CsvTable => "A table from CSV data",
            Self::Toctree => "Table of contents tree",
            Self::Include => "Include another file",
            Self::Math => "A math block (LaTeX)",
            Self::Glossary => "A glossary of terms",
        }
    }

    /// Returns the snippet format for this directive.
    pub fn snippet(&self, fence_char: char, fence_count: usize) -> String {
        let fence: String = std::iter::repeat_n(fence_char, fence_count).collect();

        match self {
            // Directives with arguments
            Self::Admonition => format!(
                "{fence}{{{name}}} ${{1:Title}}\n${{2:Content}}\n{fence}",
                fence = fence,
                name = self.name()
            ),
            Self::CodeBlock => format!(
                "{fence}{{{name}}} ${{1:python}}\n${{2:# code}}\n{fence}",
                fence = fence,
                name = self.name()
            ),
            Self::LiteralInclude => format!(
                "{fence}{{{name}}} ${{1:path/to/file}}\n{fence}",
                fence = fence,
                name = self.name()
            ),
            Self::Figure | Self::Image => format!(
                "{fence}{{{name}}} ${{1:path/to/image}}\n:alt: ${{2:description}}\n{fence}",
                fence = fence,
                name = self.name()
            ),
            Self::Include => format!(
                "{fence}{{{name}}} ${{1:path/to/file.md}}\n{fence}",
                fence = fence,
                name = self.name()
            ),
            // Directives without arguments (simple content)
            _ => format!(
                "{fence}{{{name}}}\n${{1:Content}}\n{fence}",
                fence = fence,
                name = self.name()
            ),
        }
    }
}

impl<'a> Completable<'a, MystDirectiveCompleter> for MystDirective {
    fn completions(&self, completer: &MystDirectiveCompleter) -> Option<CompletionItem> {
        let snippet = self.snippet(completer.fence_char, completer.fence_count);
        let filter_text = completer.completion_filter_text(self.name());

        // The text edit should replace from the fence start to the current cursor
        let completion_item = CompletionItem {
            label: self.name().to_string(),
            detail: Some(self.description().to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            kind: Some(CompletionItemKind::SNIPPET),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: Range {
                    start: Position {
                        line: completer.line,
                        character: completer.fence_start as u32,
                    },
                    end: Position {
                        line: completer.line,
                        character: completer.character,
                    },
                },
                new_text: snippet,
            })),
            filter_text: Some(filter_text),
            ..Default::default()
        };

        Some(completion_item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Test helpers
    // =========================================================================

    /// Creates a minimal test context for pattern matching tests.
    /// These tests focus on regex pattern matching, not vault integration.

    // =========================================================================
    // RED PHASE: Tests for fence pattern detection
    // =========================================================================

    mod pattern_detection {
        use super::*;

        #[test]
        fn test_backtick_fence_with_open_brace() {
            // Test: ```{
            let input = "```{";
            let result = parse_fence_pattern(input, input.len());
            assert!(
                result.is_some(),
                "Should match backtick fence with open brace"
            );
            let (fence_char, fence_count, partial) = result.unwrap();
            assert_eq!(fence_char, '`');
            assert_eq!(fence_count, 3);
            assert_eq!(partial, "");
        }

        #[test]
        fn test_backtick_fence_with_partial_directive() {
            // Test: ```{no
            let input = "```{no";
            let result = parse_fence_pattern(input, input.len());
            assert!(
                result.is_some(),
                "Should match backtick fence with partial directive"
            );
            let (fence_char, fence_count, partial) = result.unwrap();
            assert_eq!(fence_char, '`');
            assert_eq!(fence_count, 3);
            assert_eq!(partial, "no");
        }

        #[test]
        fn test_colon_fence_with_open_brace() {
            // Test: :::{
            let input = ":::{";
            let result = parse_fence_pattern(input, input.len());
            assert!(result.is_some(), "Should match colon fence with open brace");
            let (fence_char, fence_count, partial) = result.unwrap();
            assert_eq!(fence_char, ':');
            assert_eq!(fence_count, 3);
            assert_eq!(partial, "");
        }

        #[test]
        fn test_colon_fence_with_partial_directive() {
            // Test: :::{warn
            let input = ":::{warn";
            let result = parse_fence_pattern(input, input.len());
            assert!(
                result.is_some(),
                "Should match colon fence with partial directive"
            );
            let (fence_char, fence_count, partial) = result.unwrap();
            assert_eq!(fence_char, ':');
            assert_eq!(fence_count, 3);
            assert_eq!(partial, "warn");
        }

        #[test]
        fn test_longer_backtick_fence() {
            // Test: ````{tip
            let input = "````{tip";
            let result = parse_fence_pattern(input, input.len());
            assert!(result.is_some(), "Should match longer backtick fence");
            let (fence_char, fence_count, partial) = result.unwrap();
            assert_eq!(fence_char, '`');
            assert_eq!(fence_count, 4);
            assert_eq!(partial, "tip");
        }

        #[test]
        fn test_longer_colon_fence() {
            // Test: ::::{note
            let input = "::::{note";
            let result = parse_fence_pattern(input, input.len());
            assert!(result.is_some(), "Should match longer colon fence");
            let (fence_char, fence_count, partial) = result.unwrap();
            assert_eq!(fence_char, ':');
            assert_eq!(fence_count, 4);
            assert_eq!(partial, "note");
        }

        #[test]
        fn test_fence_with_leading_whitespace() {
            // Test:    ```{code
            let input = "   ```{code";
            let result = parse_fence_pattern(input, input.len());
            assert!(
                result.is_some(),
                "Should match fence with leading whitespace"
            );
            let (fence_char, fence_count, partial) = result.unwrap();
            assert_eq!(fence_char, '`');
            assert_eq!(fence_count, 3);
            assert_eq!(partial, "code");
        }

        #[test]
        fn test_directive_with_hyphen() {
            // Test: ```{code-block
            let input = "```{code-block";
            let result = parse_fence_pattern(input, input.len());
            assert!(result.is_some(), "Should match directive name with hyphen");
            let (_, _, partial) = result.unwrap();
            assert_eq!(partial, "code-block");
        }

        #[test]
        fn test_directive_with_underscore() {
            // Test: ```{list_table
            let input = "```{list_table";
            let result = parse_fence_pattern(input, input.len());
            assert!(
                result.is_some(),
                "Should match directive name with underscore"
            );
            let (_, _, partial) = result.unwrap();
            assert_eq!(partial, "list_table");
        }
    }

    mod pattern_rejection {
        use super::*;

        #[test]
        fn test_reject_plain_text() {
            let input = "This is plain text";
            let result = parse_fence_pattern(input, input.len());
            assert!(result.is_none(), "Should not match plain text");
        }

        #[test]
        fn test_reject_fence_without_brace() {
            // Test: ``` (no opening brace)
            let input = "```";
            let result = parse_fence_pattern(input, input.len());
            assert!(
                result.is_none(),
                "Should not match fence without opening brace"
            );
        }

        #[test]
        fn test_reject_short_fence() {
            // Test: ``{ (only 2 backticks)
            let input = "``{";
            let result = parse_fence_pattern(input, input.len());
            assert!(
                result.is_none(),
                "Should not match fence with only 2 characters"
            );
        }

        #[test]
        fn test_reject_closed_directive() {
            // Test: ```{note} (already closed)
            let input = "```{note}";
            let result = parse_fence_pattern(input, input.len());
            assert!(
                result.is_none(),
                "Should not match already closed directive"
            );
        }

        #[test]
        fn test_reject_directive_with_content_after() {
            // Test: ```{note} content
            let input = "```{note} content";
            let result = parse_fence_pattern(input, input.len());
            assert!(
                result.is_none(),
                "Should not match directive with content after"
            );
        }

        #[test]
        fn test_reject_inline_code() {
            // Test: `code`
            let input = "`code`";
            let result = parse_fence_pattern(input, input.len());
            assert!(result.is_none(), "Should not match inline code");
        }

        #[test]
        fn test_reject_markdown_link() {
            // Test: [link](url)
            let input = "[link](url)";
            let result = parse_fence_pattern(input, input.len());
            assert!(result.is_none(), "Should not match markdown link");
        }
    }

    // =========================================================================
    // RED PHASE: Tests for directive completions
    // =========================================================================

    mod directive_completions {
        use super::*;

        #[test]
        fn test_all_directives_have_names() {
            for directive in MystDirective::all() {
                assert!(!directive.name().is_empty(), "Directive should have a name");
            }
        }

        #[test]
        fn test_all_directives_have_descriptions() {
            for directive in MystDirective::all() {
                assert!(
                    !directive.description().is_empty(),
                    "Directive {:?} should have a description",
                    directive
                );
            }
        }

        #[test]
        fn test_admonition_directives_present() {
            let directives = MystDirective::all();
            let names: Vec<&str> = directives.iter().map(|d| d.name()).collect();

            // Check all admonition types are present
            assert!(names.contains(&"note"), "Should have note directive");
            assert!(
                names.contains(&"important"),
                "Should have important directive"
            );
            assert!(names.contains(&"hint"), "Should have hint directive");
            assert!(names.contains(&"tip"), "Should have tip directive");
            assert!(names.contains(&"seealso"), "Should have seealso directive");
            assert!(
                names.contains(&"attention"),
                "Should have attention directive"
            );
            assert!(names.contains(&"caution"), "Should have caution directive");
            assert!(names.contains(&"warning"), "Should have warning directive");
            assert!(names.contains(&"danger"), "Should have danger directive");
            assert!(names.contains(&"error"), "Should have error directive");
            assert!(
                names.contains(&"admonition"),
                "Should have admonition directive"
            );
        }

        #[test]
        fn test_code_directives_present() {
            let directives = MystDirective::all();
            let names: Vec<&str> = directives.iter().map(|d| d.name()).collect();

            assert!(
                names.contains(&"code-block"),
                "Should have code-block directive"
            );
            assert!(
                names.contains(&"literalinclude"),
                "Should have literalinclude directive"
            );
        }

        #[test]
        fn test_structural_directives_present() {
            let directives = MystDirective::all();
            let names: Vec<&str> = directives.iter().map(|d| d.name()).collect();

            assert!(names.contains(&"toctree"), "Should have toctree directive");
            assert!(names.contains(&"include"), "Should have include directive");
        }

        #[test]
        fn test_directive_count() {
            // We expect exactly 22 directives
            assert_eq!(
                MystDirective::all().len(),
                22,
                "Should have exactly 22 directives"
            );
        }
    }

    // =========================================================================
    // RED PHASE: Tests for snippet generation
    // =========================================================================

    mod snippet_generation {
        use super::*;

        #[test]
        fn test_simple_admonition_snippet() {
            let snippet = MystDirective::Note.snippet('`', 3);
            assert!(
                snippet.starts_with("```{note}"),
                "Snippet should start with fence and directive"
            );
            assert!(
                snippet.ends_with("```"),
                "Snippet should end with closing fence"
            );
            assert!(
                snippet.contains("${1:Content}"),
                "Snippet should have content placeholder"
            );
        }

        #[test]
        fn test_admonition_with_title_snippet() {
            let snippet = MystDirective::Admonition.snippet('`', 3);
            assert!(
                snippet.contains("${1:Title}"),
                "Admonition snippet should have title placeholder"
            );
            assert!(
                snippet.contains("${2:Content}"),
                "Admonition snippet should have content placeholder"
            );
        }

        #[test]
        fn test_code_block_snippet() {
            let snippet = MystDirective::CodeBlock.snippet('`', 3);
            assert!(
                snippet.contains("```{code-block}"),
                "Code block snippet should have directive"
            );
            assert!(
                snippet.contains("${1:python}"),
                "Code block snippet should have language placeholder"
            );
        }

        #[test]
        fn test_colon_fence_snippet() {
            let snippet = MystDirective::Note.snippet(':', 3);
            assert!(
                snippet.starts_with(":::{note}"),
                "Colon fence snippet should start with :::"
            );
            assert!(
                snippet.ends_with(":::"),
                "Colon fence snippet should end with :::"
            );
        }

        #[test]
        fn test_longer_fence_snippet() {
            let snippet = MystDirective::Warning.snippet('`', 4);
            assert!(
                snippet.starts_with("````{warning}"),
                "Longer fence snippet should have 4 backticks"
            );
            assert!(
                snippet.ends_with("````"),
                "Longer fence snippet should end with 4 backticks"
            );
        }

        #[test]
        fn test_figure_snippet_has_options() {
            let snippet = MystDirective::Figure.snippet('`', 3);
            assert!(
                snippet.contains(":alt:"),
                "Figure snippet should have alt option"
            );
        }
    }

    // =========================================================================
    // Helper function for pattern-only tests (no vault needed)
    // =========================================================================

    /// Parse a fence pattern from text, returning (fence_char, fence_count, partial_directive)
    fn parse_fence_pattern(text: &str, cursor_pos: usize) -> Option<(char, usize, String)> {
        use regex::Regex;

        static FENCE_DIRECTIVE_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(?<leading>\s*)(?<fence>(`{3,}|:{3,}))\{(?<partial>[a-zA-Z0-9_-]*)$")
                .unwrap()
        });

        let text_before_cursor = if cursor_pos <= text.len() {
            &text[..cursor_pos]
        } else {
            text
        };

        let captures = FENCE_DIRECTIVE_PATTERN.captures(text_before_cursor)?;

        let fence = captures.name("fence")?;
        let partial = captures.name("partial")?;

        let fence_str = fence.as_str();
        let fence_char = fence_str.chars().next()?;
        let fence_count = fence_str.len();

        Some((fence_char, fence_count, partial.as_str().to_string()))
    }
}
