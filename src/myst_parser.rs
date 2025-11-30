use markdown::{to_mdast, ParseOptions, mdast::Node};
use regex::Regex;

#[derive(Debug, PartialEq)]
pub enum MystNode {
    Directive {
        name: String,
        args: Option<String>,
        body: String,
    },
    // We will add more variants (Role, etc.) later
    Unimplemented,
}

pub fn parse(text: &str) -> Vec<MystNode> {
    let ast = to_mdast(text, &ParseOptions::default()).expect("Failed to parse markdown");
    let mut nodes = vec![];

    traverse(&ast, &mut nodes);
    
    nodes
}

fn traverse(node: &Node, nodes: &mut Vec<MystNode>) {
    match node {
        Node::Root(root) => {
            for child in &root.children {
                traverse(child, nodes);
            }
        },
        Node::Code(code) => {
            // Check for MyST directive syntax in the info string (lang + meta)
            // Pattern: ```{name} args
            // markdown-rs puts "{name}" into `lang` and "args" into `meta` usually, 
            // or if it's just ```{name}, lang is "{name}".
            
            if let Some(lang) = &code.lang {
                let re = Regex::new(r"^\{([a-zA-Z0-9_-]+)\}$").unwrap();
                if let Some(caps) = re.captures(lang) {
                    let name = caps.get(1).unwrap().as_str().to_string();
                    let body = code.value.clone();
                    let args = code.meta.clone();

                    nodes.push(MystNode::Directive {
                        name,
                        args,
                        body,
                    });
                }
            }
        },
        _ => {
            if let Some(children) = node.children() {
                for child in children {
                    traverse(child, nodes);
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_directive() {
        // The input: A standard MyST directive block
        let input = r#"
```{note}
This is the body of the note.
```
"#;

        // The Action: Parse it
        let nodes = parse(input);

        // The Assertion (The "Red" State): 
        // We expect a single Directive node, not a generic CodeBlock
        assert_eq!(nodes.len(), 1);
        
        match &nodes[0] {
            MystNode::Directive { name, args, body } => {
                assert_eq!(name, "note");
                assert_eq!(*args, None);
                assert_eq!(body.trim(), "This is the body of the note.");
            },
            _ => panic!("Expected a Directive node, got {:?}", nodes[0]),
        }
    }
}
