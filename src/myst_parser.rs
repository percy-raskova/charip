use markdown::{to_mdast, ParseOptions, mdast::Node};

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct MystSymbol {
    pub kind: String, // "directive", "anchor", "reference"
    pub name: String,
    pub line: usize,
}

pub fn parse(text: &str) -> Vec<MystSymbol> {
    // We might need to configure ParseOptions to be more permissive or GFM-like if needed
    let options = ParseOptions::default();
    
    match to_mdast(text, &options) {
        Ok(ast) => {
            let mut symbols = vec![];
            scan_for_myst(&ast, &mut symbols);
            symbols
        },
        Err(_) => vec![] // Fail gracefully
    }
}

pub fn scan_for_myst(node: &Node, symbols: &mut Vec<MystSymbol>) {
    // RECURSE FIRST: Go deep into the tree
    if let Some(children) = node.children() {
        for child in children {
            scan_for_myst(child, symbols);
        }
    }

    // PROCESS CURRENT NODE
    match node {
        // CASE 1: Directives (```{note})
        Node::Code(code) => {
            if let Some(lang) = &code.lang {
                 if lang.starts_with('{') && lang.ends_with('}') {
                    let directive = lang.trim_matches(|c| c == '{' || c == '}');
                    symbols.push(MystSymbol {
                        kind: "directive".to_string(),
                        name: directive.to_string(),
                        line: code.position.as_ref().map(|p| p.start.line).unwrap_or(0),
                    });
                }
            }
        },
        // CASE 2: Targets ( (my-target)= )
        Node::Text(text) => {
             // Basic detection for (target)= at the start/end of a text node
             // This is a heuristic as markdown parsers might split text
             let val = text.value.trim();
             if val.starts_with('(') && val.ends_with(")=") {
                 let target = val
                   .trim_start_matches('(')
                   .trim_end_matches(")=");
                 
                 if !target.is_empty() {
                     symbols.push(MystSymbol {
                        kind: "anchor".to_string(),
                        name: target.to_string(),
                        line: text.position.as_ref().map(|p| p.start.line).unwrap_or(0),
                    });
                 }
            }
        },
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_directive() {
        let input = r#"
```{note}
This is the body of the note.
```
"#;
        let symbols = parse(input);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].kind, "directive");
        assert_eq!(symbols[0].name, "note");
    }

    #[test]
    fn test_parse_anchor() {
        let input = "(my-target)=";
        let symbols = parse(input);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].kind, "anchor");
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
    let mut symbols = Vec::new();
    scan_for_myst(&ast, &mut symbols);

    assert!(symbols.iter().any(|s| s.kind == "directive" && s.name == "note"));
}

#[test]
fn test_finds_myst_anchor() {
    let input = "Here is a text.\n\n(my-target)=\n# Heading";
    
    let ast = markdown::to_mdast(input, &markdown::ParseOptions::default()).unwrap();
    let mut symbols = Vec::new();
    scan_for_myst(&ast, &mut symbols);

    assert!(symbols.iter().any(|s| s.kind == "anchor" && s.name == "my-target"));
}
}