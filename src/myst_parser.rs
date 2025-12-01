use markdown::{mdast::Node, to_mdast, ParseOptions};
use ropey::Rope;

use crate::vault::MyRange;

/// Type-safe representation of MyST symbol kinds.
/// Using an enum prevents typos and enables compile-time checking.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MystSymbolKind {
    Directive,
    Anchor,
    Reference, // Placeholder for future MyST cross-refs like {ref}`target`
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
}
