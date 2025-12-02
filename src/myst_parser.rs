use markdown::{mdast::Node, to_mdast, ParseOptions};
use ropey::Rope;

use crate::vault::MyRange;

/// A glossary term extracted from a `{glossary}` directive.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct GlossaryTerm {
    /// The term name (e.g., "MyST")
    pub term: String,
    /// The definition text
    pub definition: String,
    /// LSP-compatible range for go-to-definition navigation
    pub range: MyRange,
}

/// Type-safe representation of MyST symbol kinds.
/// Using an enum prevents typos and enables compile-time checking.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MystSymbolKind {
    Directive,
    Anchor,
    #[allow(dead_code)] // Placeholder for future MyST cross-refs like {ref}`target`
    Reference,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct MystSymbol {
    pub kind: MystSymbolKind,
    pub name: String,
    pub line: usize,
    /// LSP-compatible range for go-to-definition navigation
    pub range: MyRange,
    /// Extracted from `:label:` or `:name:` directive options
    pub label: Option<String>,
}

/// Parse glossary terms from MyST document text.
///
/// Extracts terms from `{glossary}` directives. Each term is identified
/// by a line starting with non-whitespace followed by indented definition lines.
///
/// # Example
/// ```ignore
/// ```{glossary}
/// MyST
///   Markedly Structured Text, an extended Markdown syntax.
/// ```
/// ```
pub fn parse_glossary_terms(text: &str) -> Vec<GlossaryTerm> {
    let options = ParseOptions::default();
    let rope = Rope::from_str(text);

    match to_mdast(text, &options) {
        Ok(ast) => {
            let mut terms = vec![];
            extract_glossary_terms(&ast, &rope, &mut terms);
            terms
        }
        Err(_) => vec![],
    }
}

/// Extract glossary terms from AST nodes.
fn extract_glossary_terms(node: &Node, rope: &Rope, terms: &mut Vec<GlossaryTerm>) {
    // Recurse into children first
    if let Some(children) = node.children() {
        for child in children {
            extract_glossary_terms(child, rope, terms);
        }
    }

    // Look for code blocks with {glossary} language
    if let Node::Code(code) = node {
        if let Some(lang) = &code.lang {
            if lang.starts_with('{') && lang.ends_with('}') {
                let directive = lang.trim_matches(|c| c == '{' || c == '}');
                if directive == "glossary" {
                    // Parse the glossary content
                    let content = &code.value;
                    let base_line = code.position.as_ref().map(|p| p.start.line).unwrap_or(0);

                    parse_glossary_content(content, base_line, rope, terms);
                }
            }
        }
    }
}

/// Parse the content of a glossary directive to extract terms.
///
/// Glossary format:
/// ```text
/// Term Name
///   Definition line 1
///   Definition line 2
///
/// Another Term
///   Another definition
/// ```
fn parse_glossary_content(
    content: &str,
    base_line: usize,
    rope: &Rope,
    terms: &mut Vec<GlossaryTerm>,
) {
    let mut current_term: Option<String> = None;
    let mut current_definition: Vec<String> = vec![];
    let mut term_line: usize = 0;

    // Helper closure to save the current term and clear state.
    // This eliminates duplication between the mid-parse save and final save.
    let save_current_term = |term: String,
                             definition: &mut Vec<String>,
                             saved_line: usize,
                             terms: &mut Vec<GlossaryTerm>| {
        let definition_text = definition.join(" ");
        // base_line is from markdown-rs (1-indexed). Adding term_line gives
        // the 0-indexed LSP line number for this term.
        let range = calculate_term_range(base_line + saved_line, &term, rope);
        terms.push(GlossaryTerm {
            term,
            definition: definition_text,
            range,
        });
        definition.clear();
    };

    for (line_idx, line) in content.lines().enumerate() {
        // Skip directive option lines like `:sorted:`
        if line.trim_start().starts_with(':') && line.trim_start().contains(':') {
            let trimmed = line.trim();
            if trimmed.starts_with(':') && trimmed.ends_with(':') {
                continue;
            }
        }

        let is_indented = line.starts_with(' ') || line.starts_with('\t');
        let is_empty = line.trim().is_empty();

        if is_empty {
            continue;
        }

        if is_indented {
            // Definition line: append to current term's definition
            if current_term.is_some() {
                current_definition.push(line.trim().to_string());
            }
        } else {
            // New term line: save previous term if any, then start new term
            if let Some(term) = current_term.take() {
                save_current_term(term, &mut current_definition, term_line, terms);
            }
            current_term = Some(line.trim().to_string());
            term_line = line_idx;
        }
    }

    // Save the final term (the loop only saves when encountering a new term)
    if let Some(term) = current_term {
        save_current_term(term, &mut current_definition, term_line, terms);
    }
}

/// Calculate the LSP range for a glossary term.
///
/// Currently uses a simplified approach: the range spans from column 0 to
/// the term's character length. This works well for typical glossary entries
/// where terms start at the beginning of a line.
///
/// The `_rope` parameter is accepted for API consistency with other range
/// calculation functions in the codebase, which use it for byte-offset-to-
/// line/character conversion. Future enhancements may use it for precise
/// Unicode character counting.
fn calculate_term_range(line: usize, term: &str, _rope: &Rope) -> MyRange {
    use tower_lsp::lsp_types::{Position, Range};
    MyRange(Range {
        start: Position {
            line: line as u32,
            character: 0,
        },
        end: Position {
            line: line as u32,
            character: term.len() as u32,
        },
    })
}

pub fn parse(text: &str) -> Vec<MystSymbol> {
    // We might need to configure ParseOptions to be more permissive or GFM-like if needed
    let options = ParseOptions::default();
    let rope = Rope::from_str(text);

    match to_mdast(text, &options) {
        Ok(ast) => {
            let mut symbols = vec![];
            scan_for_myst(&ast, &rope, &mut symbols);
            symbols
        }
        Err(_) => vec![], // Fail gracefully
    }
}

/// Extracts :label: or :name: from directive content.
/// :label: takes priority over :name: if both are present.
fn extract_directive_label(content: &str) -> Option<String> {
    let mut label: Option<String> = None;
    let mut name: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        // Stop at first non-option line (options must be at the start)
        if !trimmed.starts_with(':') {
            if !trimmed.is_empty() {
                break;
            }
            continue; // Skip empty lines within options section
        }

        if let Some(value) = trimmed.strip_prefix(":label:") {
            label = Some(value.trim().to_string());
        } else if let Some(value) = trimmed.strip_prefix(":name:") {
            name = Some(value.trim().to_string());
        }
    }

    // :label: takes priority over :name:
    label.or(name)
}

/// Extracts range and line from an AST position.
fn extract_position_info(
    position: &Option<markdown::unist::Position>,
    rope: &Rope,
) -> (MyRange, usize) {
    let range = position
        .as_ref()
        .map(|p| MyRange::from_range(rope, p.start.offset..p.end.offset))
        .unwrap_or_default();
    let line = position.as_ref().map(|p| p.start.line).unwrap_or(0);
    (range, line)
}

pub fn scan_for_myst(node: &Node, rope: &Rope, symbols: &mut Vec<MystSymbol>) {
    // RECURSE FIRST: Go deep into the tree
    if let Some(children) = node.children() {
        for child in children {
            scan_for_myst(child, rope, symbols);
        }
    }

    // PROCESS CURRENT NODE
    match node {
        // CASE 1: Directives (```{note})
        Node::Code(code) => {
            if let Some(lang) = &code.lang {
                if lang.starts_with('{') && lang.ends_with('}') {
                    let directive = lang.trim_matches(|c| c == '{' || c == '}');
                    let (range, line) = extract_position_info(&code.position, rope);
                    let label = extract_directive_label(&code.value);

                    symbols.push(MystSymbol {
                        kind: MystSymbolKind::Directive,
                        name: directive.to_string(),
                        line,
                        range,
                        label,
                    });
                }
            }
        }
        // CASE 2: Targets ( (my-target)= )
        Node::Text(text) => {
            // Basic detection for (target)= at the start/end of a text node
            // This is a heuristic as markdown parsers might split text
            let val = text.value.trim();
            if val.starts_with('(') && val.ends_with(")=") {
                let target = val.trim_start_matches('(').trim_end_matches(")=");

                if !target.is_empty() {
                    let (range, line) = extract_position_info(&text.position, rope);

                    symbols.push(MystSymbol {
                        kind: MystSymbolKind::Anchor,
                        name: target.to_string(),
                        line,
                        range,
                        label: None, // Anchors don't have labels
                    });
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Range field tests (Phase 2.5 Part 1)
    // ========================================================================

    #[test]
    fn test_myst_symbol_has_range_field() {
        let input = "(my-target)=";
        let symbols = parse(input);

        assert_eq!(symbols.len(), 1);
        let range = symbols[0].range;
        // Range should cover the anchor "(my-target)=" (12 chars)
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 12);
    }

    #[test]
    fn test_directive_has_range_field() {
        let input = "```{note}\nContent\n```";
        let symbols = parse(input);

        assert_eq!(symbols.len(), 1);
        let range = symbols[0].range;
        // Directive should span from line 0 to line 2
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 2);
    }

    #[test]
    fn test_anchor_range_on_non_first_line() {
        let input = "# Title\n\n(my-anchor)=\n# Section";
        let symbols = parse(input);

        let anchor = symbols
            .iter()
            .find(|s| s.kind == MystSymbolKind::Anchor)
            .expect("Should find anchor");

        // Anchor is on line 2 (0-indexed)
        assert_eq!(anchor.range.start.line, 2);
    }

    // ========================================================================
    // Directive label/name parsing tests (Phase 2.5 Part 2)
    // ========================================================================

    #[test]
    fn test_directive_with_label_option() {
        let input = r#"```{figure} image.png
:label: my-figure

Caption here
```"#;
        let symbols = parse(input);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].kind, MystSymbolKind::Directive);
        assert_eq!(symbols[0].name, "figure");
        assert_eq!(symbols[0].label, Some("my-figure".to_string()));
    }

    #[test]
    fn test_directive_with_name_option() {
        // :name: is an alias for :label: in MyST
        let input = r#"```{code-block} python
:name: hello-world
print("Hello")
```"#;
        let symbols = parse(input);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].label, Some("hello-world".to_string()));
    }

    #[test]
    fn test_directive_without_label() {
        let input = r#"```{note}
This note has no label.
```"#;
        let symbols = parse(input);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].label, None);
    }

    #[test]
    fn test_directive_label_takes_priority_over_name() {
        let input = r#"```{figure} image.png
:label: from-label
:name: from-name

Caption
```"#;
        let symbols = parse(input);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].label, Some("from-label".to_string()));
    }

    #[test]
    fn test_directive_options_with_other_options() {
        let input = r#"```{figure} image.png
:width: 80%
:label: my-figure
:align: center

Caption
```"#;
        let symbols = parse(input);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].label, Some("my-figure".to_string()));
    }

    // ========================================================================
    // Basic parsing tests
    // ========================================================================

    #[test]
    fn test_parse_basic_directive() {
        let input = r#"
```{note}
This is the body of the note.
```
"#;
        let symbols = parse(input);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].kind, MystSymbolKind::Directive);
        assert_eq!(symbols[0].name, "note");
    }

    #[test]
    fn test_parse_anchor() {
        let input = "(my-target)=";
        let symbols = parse(input);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].kind, MystSymbolKind::Anchor);
        assert_eq!(symbols[0].name, "my-target");
    }

    #[test]
    fn test_finds_myst_directive() {
        let input = r#"
# Title

```{note}
This is a note

    "#;

        let ast = markdown::to_mdast(input, &markdown::ParseOptions::default()).unwrap();
        let rope = Rope::from_str(input);
        let mut symbols = Vec::new();
        scan_for_myst(&ast, &rope, &mut symbols);

        assert!(symbols
            .iter()
            .any(|s| s.kind == MystSymbolKind::Directive && s.name == "note"));
    }

    #[test]
    fn test_finds_myst_anchor() {
        let input = "Here is a text.\n\n(my-target)=\n# Heading";

        let ast = markdown::to_mdast(input, &markdown::ParseOptions::default()).unwrap();
        let rope = Rope::from_str(input);
        let mut symbols = Vec::new();
        scan_for_myst(&ast, &rope, &mut symbols);

        assert!(symbols
            .iter()
            .any(|s| s.kind == MystSymbolKind::Anchor && s.name == "my-target"));
    }

    // ========================================================================
    // Glossary term extraction tests (RED PHASE)
    // ========================================================================

    #[test]
    fn test_parse_glossary_directive_extracts_terms() {
        let input = r#"```{glossary}
MyST
  Markedly Structured Text, an extended Markdown syntax.

Sphinx
  A documentation generator for Python projects.
```"#;
        let terms = parse_glossary_terms(input);

        assert_eq!(terms.len(), 2);
        assert!(terms.iter().any(|t| t.term == "MyST"));
        assert!(terms.iter().any(|t| t.term == "Sphinx"));
    }

    #[test]
    fn test_glossary_term_has_definition() {
        let input = r#"```{glossary}
MyST
  Markedly Structured Text, an extended Markdown syntax.
```"#;
        let terms = parse_glossary_terms(input);

        assert_eq!(terms.len(), 1);
        let term = &terms[0];
        assert_eq!(term.term, "MyST");
        assert!(term.definition.contains("Markedly Structured Text"));
    }

    #[test]
    fn test_glossary_term_has_range() {
        let input = r#"```{glossary}
MyST
  Definition here.
```"#;
        let terms = parse_glossary_terms(input);

        assert_eq!(terms.len(), 1);
        let term = &terms[0];
        // Term "MyST" is on line 1 (0-indexed)
        assert_eq!(term.range.start.line, 1);
    }

    #[test]
    fn test_glossary_multiline_definition() {
        let input = r#"```{glossary}
Term
  First line of definition.
  Second line of definition.

Another Term
  Another definition.
```"#;
        let terms = parse_glossary_terms(input);

        assert_eq!(terms.len(), 2);
        let term = terms.iter().find(|t| t.term == "Term").unwrap();
        assert!(term.definition.contains("First line"));
        assert!(term.definition.contains("Second line"));
    }

    #[test]
    fn test_glossary_term_with_sorted_option() {
        // Glossary can have :sorted: option which we should ignore when parsing terms
        let input = r#"```{glossary}
:sorted:

Alpha
  First term alphabetically.

Beta
  Second term.
```"#;
        let terms = parse_glossary_terms(input);

        assert_eq!(terms.len(), 2);
        assert!(terms.iter().any(|t| t.term == "Alpha"));
        assert!(terms.iter().any(|t| t.term == "Beta"));
    }

    #[test]
    fn test_no_glossary_terms_in_regular_directive() {
        let input = r#"```{note}
This is not a glossary.
```"#;
        let terms = parse_glossary_terms(input);

        assert!(terms.is_empty());
    }

    #[test]
    fn test_glossary_terms_in_document_with_multiple_directives() {
        let input = r#"# Introduction

```{note}
A note before the glossary.
```

```{glossary}
Term1
  First term.

Term2
  Second term.
```

```{warning}
A warning after.
```"#;
        let terms = parse_glossary_terms(input);

        assert_eq!(terms.len(), 2);
        assert!(terms.iter().any(|t| t.term == "Term1"));
        assert!(terms.iter().any(|t| t.term == "Term2"));
    }
}
