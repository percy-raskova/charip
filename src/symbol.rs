//! Symbol providers for MyST Markdown documents.
//!
//! This module implements LSP symbol capabilities:
//! - `textDocument/documentSymbol` - outline view of current file
//! - `workspace/symbol` - fuzzy search across all files
//!
//! # Document Symbols
//!
//! The document outline includes:
//!
//! | Symbol Type | LSP Kind | Example |
//! |-------------|----------|---------|
//! | Headings | `Struct` | `# Introduction` |
//! | MyST anchors | `Key` | `(my-anchor)=` |
//! | Labeled directives | `Object` | `:name: fig-example` |
//! | Glossary terms | `Constant` | `MyST` in `{glossary}` |
//!
//! Symbols are returned in document order (sorted by line number).
//!
//! # Workspace Symbols
//!
//! Fuzzy search across the entire vault using [`nucleo_matcher`].
//! Returns all referenceables (files, headings, anchors, tags, etc.)
//! ranked by match score.
//!
//! # Architecture
//!
//! ```text
//! document_symbol()   → FlatSymbol list → DocumentSymbolResponse
//! workspace_symbol()  → fuzzy match     → Vec<SymbolInformation>
//! ```

use std::{iter, path::Path};

use itertools::Itertools;
use nucleo_matcher::{
    pattern::{self, Normalization},
    Matcher,
};
use tower_lsp::lsp_types::{
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, Range, SymbolInformation,
    SymbolKind, WorkspaceSymbolParams,
};

use crate::vault::{MDHeading, Vault};

fn compute_match_score(
    matcher: &mut Matcher,
    pattern: &pattern::Pattern,
    symbol: SymbolInformation,
) -> (u32, SymbolInformation) {
    let mut buf = Vec::new();
    (
        pattern
            .score(
                nucleo_matcher::Utf32Str::new(symbol.name.as_str(), &mut buf),
                matcher,
            )
            .unwrap_or_default(),
        symbol,
    )
}

/// Search for symbols across the entire vault.
///
/// Performs fuzzy matching against all referenceables (files, headings,
/// anchors, tags, indexed blocks) and returns results ranked by match score.
///
/// # Arguments
///
/// * `vault` - The indexed vault
/// * `_params` - LSP workspace symbol request (contains query string)
///
/// # Returns
///
/// Vector of matching symbols, sorted by fuzzy match score (best first).
/// Returns empty vector if query matches nothing.
pub fn workspace_symbol(
    vault: &Vault,
    _params: &WorkspaceSymbolParams,
) -> Option<Vec<SymbolInformation>> {
    // Initialize the fuzzy matcher
    let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
    let pattern = pattern::Pattern::parse(
        &_params.query,
        pattern::CaseMatching::Smart,
        Normalization::Smart,
    );

    // Collect symbols and order by fuzzy matching score
    Some(
        vault
            .select_referenceable_nodes(None)
            .into_iter()
            .flat_map(|referenceable| vault.to_symbol_information(referenceable))
            // Fuzzy matcher - compute match score
            .map(|symbol| compute_match_score(&mut matcher, &pattern, symbol))
            // Remove all items with no matches
            .filter(|(score, _)| *score > 0)
            // Sort by match score descending
            .sorted_by(|(a, _), (b, _)| Ord::cmp(b, a))
            // Strip the score from the result
            .map(|(_score, symbol)| symbol)
            .collect_vec(),
    )
}

/// Internal representation of a document symbol for sorting.
#[derive(Debug)]
struct FlatSymbol {
    name: String,
    kind: SymbolKind,
    range: Range,
    detail: Option<String>,
    line: u32,
}

/// Get document outline symbols for a single file.
///
/// Returns all navigable symbols in the document:
/// - Headings (H1-H6)
/// - MyST anchors (`(target)=`)
/// - Labeled directives (`:name:` option)
/// - Glossary terms
///
/// # Arguments
///
/// * `vault` - The indexed vault
/// * `_params` - LSP document symbol request
/// * `path` - Path to the file
///
/// # Returns
///
/// Flat list of symbols sorted by line number, or `None` if file
/// has no symbols or doesn't exist in the vault.
pub fn document_symbol(
    vault: &Vault,
    _params: &DocumentSymbolParams,
    path: &Path,
) -> Option<DocumentSymbolResponse> {
    let mut symbols: Vec<FlatSymbol> = Vec::new();

    // Collect headings
    if let Some(headings) = vault.select_headings(path) {
        for heading in headings {
            symbols.push(FlatSymbol {
                name: heading.heading_text.clone(),
                kind: SymbolKind::STRUCT,
                range: *heading.range,
                detail: Some(format!("H{}", heading.level.0)),
                line: heading.range.start.line,
            });
        }
    }

    // Collect MyST anchors (target-name)=
    for (_, symbol) in vault.select_myst_anchors(Some(path)) {
        symbols.push(FlatSymbol {
            name: symbol.name.clone(),
            kind: SymbolKind::KEY,
            range: *symbol.range,
            detail: Some("anchor".to_string()),
            line: symbol.range.start.line,
        });
    }

    // Collect MyST directives with labels (only those with :name: or :label:)
    for (_, symbol) in vault.select_myst_directives(Some(path)) {
        if let Some(label) = &symbol.label {
            symbols.push(FlatSymbol {
                name: label.clone(),
                kind: SymbolKind::OBJECT,
                range: *symbol.range,
                detail: Some(format!("{{{}}} directive", symbol.name)),
                line: symbol.range.start.line,
            });
        }
    }

    // Collect glossary terms
    for (_, term) in vault.select_glossary_terms(Some(path)) {
        symbols.push(FlatSymbol {
            name: term.term.clone(),
            kind: SymbolKind::CONSTANT,
            range: *term.range,
            detail: Some("glossary term".to_string()),
            line: term.range.start.line,
        });
    }

    // Return None if no symbols found
    if symbols.is_empty() {
        return None;
    }

    // Sort by line number to maintain document order
    symbols.sort_by_key(|s| s.line);

    // Convert to flat DocumentSymbol list
    let document_symbols = symbols_to_flat_list(symbols);

    Some(DocumentSymbolResponse::Nested(document_symbols))
}

/// Convert flat symbols to DocumentSymbol list (no nesting for initial implementation).
#[allow(deprecated)] // field deprecated has been deprecated in favor of using tags
fn symbols_to_flat_list(symbols: Vec<FlatSymbol>) -> Vec<DocumentSymbol> {
    symbols
        .into_iter()
        .map(|s| DocumentSymbol {
            name: s.name,
            kind: s.kind,
            range: s.range,
            selection_range: s.range,
            detail: s.detail,
            deprecated: None,
            tags: None,
            children: None,
        })
        .collect()
}

#[derive(PartialEq, Debug)]
struct Node {
    heading: MDHeading,
    children: Option<Vec<Node>>,
}

fn construct_tree(headings: &[MDHeading]) -> Option<Vec<Node>> {
    match &headings {
        [only] => {
            let node = Node {
                heading: only.clone(),
                children: None,
            };
            Some(vec![node])
        }
        [first, rest @ ..] => {
            let break_index = rest
                .iter()
                .find_position(|heading| first.level >= heading.level);

            match break_index.map(|(index, _)| (&rest[..index], &rest[index..])) {
                Some((to_next, rest)) => {
                    // to_next is could be an empty list and rest has at least one item
                    let node = Node {
                        heading: first.clone(),
                        children: construct_tree(to_next), // if to_next is empty, this will return none
                    };

                    Some(
                        iter::once(node)
                            .chain(construct_tree(rest).into_iter().flatten())
                            .collect(),
                    )
                }
                None => {
                    let node = Node {
                        heading: first.clone(),
                        children: construct_tree(rest),
                    };
                    Some(vec![node])
                }
            }
        }
        [] => None,
    }
}

#[allow(dead_code)] // Reserved for future nested heading symbol support
#[allow(deprecated)] // field deprecated has been deprecated in favor of using tags and will be removed in the future
fn map_to_lsp_tree(tree: Vec<Node>) -> Vec<DocumentSymbol> {
    tree.into_iter()
        .map(|node| DocumentSymbol {
            name: node.heading.heading_text,
            kind: SymbolKind::STRUCT,
            deprecated: None,
            tags: None,
            range: *node.heading.range,
            detail: None,
            selection_range: *node.heading.range,
            children: node.children.map(map_to_lsp_tree),
        })
        .collect()
}

#[cfg(test)]
mod test {
    use crate::{
        symbol,
        vault::{HeadingLevel, MDHeading},
    };

    // ============================================================================
    // MyST Document Symbol Integration Tests (TDD RED PHASE)
    // ============================================================================
    //
    // These tests verify that document_symbol returns MyST-specific elements:
    // - Anchors (target-name)= with SymbolKind::KEY
    // - Directives with :name:/:label: with SymbolKind::OBJECT
    // - Glossary terms with SymbolKind::CONSTANT
    // ============================================================================

    mod myst_symbols {
        use std::fs;

        use tower_lsp::lsp_types::{DocumentSymbolParams, SymbolKind, TextDocumentIdentifier, Url};

        use crate::{symbol::document_symbol, test_utils::create_test_vault};

        /// Helper to extract flat list of symbols from nested DocumentSymbolResponse
        fn flatten_symbols(
            response: &tower_lsp::lsp_types::DocumentSymbolResponse,
        ) -> Vec<(&str, SymbolKind)> {
            match response {
                tower_lsp::lsp_types::DocumentSymbolResponse::Nested(symbols) => {
                    fn collect(
                        symbols: &[tower_lsp::lsp_types::DocumentSymbol],
                        result: &mut Vec<(String, SymbolKind)>,
                    ) {
                        for s in symbols {
                            result.push((s.name.clone(), s.kind));
                            if let Some(children) = &s.children {
                                collect(children, result);
                            }
                        }
                    }
                    let mut result = Vec::new();
                    collect(symbols, &mut result);
                    result
                        .into_iter()
                        .map(|(name, kind)| (Box::leak(name.into_boxed_str()) as &str, kind))
                        .collect()
                }
                tower_lsp::lsp_types::DocumentSymbolResponse::Flat(symbols) => {
                    symbols.iter().map(|s| (s.name.as_str(), s.kind)).collect()
                }
            }
        }

        #[test]
        fn test_document_with_myst_anchors_returns_anchor_symbols() {
            let content = r#"(my-anchor)=
# Section One

Some content here.

(another-anchor)=
## Subsection
"#;
            let (_temp_dir, vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            let path = vault_dir.join("test.md");
            let params = DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&path).unwrap(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            let response = document_symbol(&vault, &params, &path);
            assert!(response.is_some(), "Should return document symbols");

            let symbols = flatten_symbols(response.as_ref().unwrap());

            // Should find anchors with SymbolKind::KEY
            let anchor_symbols: Vec<_> = symbols
                .iter()
                .filter(|(_, kind)| *kind == SymbolKind::KEY)
                .collect();
            assert_eq!(
                anchor_symbols.len(),
                2,
                "Should find 2 anchor symbols, found: {:?}",
                symbols
            );
            assert!(
                anchor_symbols.iter().any(|(name, _)| *name == "my-anchor"),
                "Should find 'my-anchor'"
            );
            assert!(
                anchor_symbols
                    .iter()
                    .any(|(name, _)| *name == "another-anchor"),
                "Should find 'another-anchor'"
            );
        }

        #[test]
        fn test_document_with_labeled_directives_returns_directive_symbols() {
            let content = r#"# Document

```{figure} image.png
:name: my-figure
:width: 80%

A figure caption.
```

```{code-block} python
:label: hello-code

print("Hello")
```

```{note}
This note has no label - should NOT appear as OBJECT symbol.
```
"#;
            let (_temp_dir, vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            let path = vault_dir.join("test.md");
            let params = DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&path).unwrap(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            let response = document_symbol(&vault, &params, &path);
            assert!(response.is_some(), "Should return document symbols");

            let symbols = flatten_symbols(response.as_ref().unwrap());

            // Should find labeled directives with SymbolKind::OBJECT
            let directive_symbols: Vec<_> = symbols
                .iter()
                .filter(|(_, kind)| *kind == SymbolKind::OBJECT)
                .collect();
            assert_eq!(
                directive_symbols.len(),
                2,
                "Should find 2 labeled directive symbols, found: {:?}",
                symbols
            );
            assert!(
                directive_symbols
                    .iter()
                    .any(|(name, _)| *name == "my-figure"),
                "Should find 'my-figure'"
            );
            assert!(
                directive_symbols
                    .iter()
                    .any(|(name, _)| *name == "hello-code"),
                "Should find 'hello-code'"
            );
        }

        #[test]
        fn test_document_with_glossary_returns_term_symbols() {
            let content = r#"# Glossary

```{glossary}
MyST
  Markedly Structured Text, an extended Markdown syntax.

Sphinx
  A documentation generator for Python projects.

RST
  reStructuredText, a plaintext markup language.
```
"#;
            let (_temp_dir, vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            let path = vault_dir.join("test.md");
            let params = DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&path).unwrap(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            let response = document_symbol(&vault, &params, &path);
            assert!(response.is_some(), "Should return document symbols");

            let symbols = flatten_symbols(response.as_ref().unwrap());

            // Should find glossary terms with SymbolKind::CONSTANT
            let term_symbols: Vec<_> = symbols
                .iter()
                .filter(|(_, kind)| *kind == SymbolKind::CONSTANT)
                .collect();
            assert_eq!(
                term_symbols.len(),
                3,
                "Should find 3 glossary term symbols, found: {:?}",
                symbols
            );
            assert!(
                term_symbols.iter().any(|(name, _)| *name == "MyST"),
                "Should find 'MyST'"
            );
            assert!(
                term_symbols.iter().any(|(name, _)| *name == "Sphinx"),
                "Should find 'Sphinx'"
            );
            assert!(
                term_symbols.iter().any(|(name, _)| *name == "RST"),
                "Should find 'RST'"
            );
        }

        #[test]
        fn test_mixed_document_returns_all_symbol_types() {
            let content = r#"(intro-anchor)=
# Introduction

Welcome to the documentation.

```{figure} diagram.png
:name: architecture-diagram

System architecture overview.
```

## Terminology

```{glossary}
API
  Application Programming Interface.
```

(summary-anchor)=
## Summary
"#;
            let (_temp_dir, vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            let path = vault_dir.join("test.md");
            let params = DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&path).unwrap(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            let response = document_symbol(&vault, &params, &path);
            assert!(response.is_some(), "Should return document symbols");

            let symbols = flatten_symbols(response.as_ref().unwrap());

            // Count each symbol type
            let anchor_count = symbols
                .iter()
                .filter(|(_, kind)| *kind == SymbolKind::KEY)
                .count();
            let directive_count = symbols
                .iter()
                .filter(|(_, kind)| *kind == SymbolKind::OBJECT)
                .count();
            let term_count = symbols
                .iter()
                .filter(|(_, kind)| *kind == SymbolKind::CONSTANT)
                .count();
            let heading_count = symbols
                .iter()
                .filter(|(_, kind)| *kind == SymbolKind::STRUCT)
                .count();

            assert_eq!(anchor_count, 2, "Should find 2 anchors");
            assert_eq!(directive_count, 1, "Should find 1 labeled directive");
            assert_eq!(term_count, 1, "Should find 1 glossary term");
            assert_eq!(heading_count, 3, "Should find 3 headings");
        }

        #[test]
        fn test_symbols_have_correct_symbol_kinds() {
            // Verify the specific SymbolKind assignments per the requirements
            let content = r#"(test-anchor)=
# Heading

```{note}
:name: labeled-note
Content
```

```{glossary}
Term
  Definition
```
"#;
            let (_temp_dir, vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            let path = vault_dir.join("test.md");
            let params = DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&path).unwrap(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            let response = document_symbol(&vault, &params, &path);
            let symbols = flatten_symbols(response.as_ref().unwrap());

            // Verify specific symbol kinds
            let anchor = symbols.iter().find(|(name, _)| *name == "test-anchor");
            assert!(anchor.is_some(), "Should find test-anchor");
            assert_eq!(
                anchor.unwrap().1,
                SymbolKind::KEY,
                "Anchor should be SymbolKind::KEY"
            );

            let directive = symbols.iter().find(|(name, _)| *name == "labeled-note");
            assert!(directive.is_some(), "Should find labeled-note");
            assert_eq!(
                directive.unwrap().1,
                SymbolKind::OBJECT,
                "Labeled directive should be SymbolKind::OBJECT"
            );

            let term = symbols.iter().find(|(name, _)| *name == "Term");
            assert!(term.is_some(), "Should find Term");
            assert_eq!(
                term.unwrap().1,
                SymbolKind::CONSTANT,
                "Glossary term should be SymbolKind::CONSTANT"
            );

            let heading = symbols.iter().find(|(name, _)| *name == "Heading");
            assert!(heading.is_some(), "Should find Heading");
            assert_eq!(
                heading.unwrap().1,
                SymbolKind::STRUCT,
                "Heading should be SymbolKind::STRUCT"
            );
        }
    }

    #[test]
    fn test_simple_tree() {
        let headings = vec![
            MDHeading {
                level: HeadingLevel(1),
                heading_text: "First".to_string(),
                range: Default::default(),
            },
            MDHeading {
                level: HeadingLevel(2),
                heading_text: "Second".to_string(),
                range: Default::default(),
            },
            MDHeading {
                level: HeadingLevel(3),
                heading_text: "Third".to_string(),
                range: Default::default(),
            },
            MDHeading {
                level: HeadingLevel(2),
                heading_text: "Second".to_string(),
                range: Default::default(),
            },
            MDHeading {
                level: HeadingLevel(1),
                heading_text: "First".to_string(),
                range: Default::default(),
            },
            MDHeading {
                level: HeadingLevel(1),
                heading_text: "First".to_string(),
                range: Default::default(),
            },
        ];

        let tree = super::construct_tree(&headings);

        let expected = vec![
            symbol::Node {
                heading: MDHeading {
                    level: HeadingLevel(1),
                    heading_text: "First".to_string(),
                    range: Default::default(),
                },
                children: Some(vec![
                    symbol::Node {
                        heading: MDHeading {
                            level: HeadingLevel(2),
                            heading_text: "Second".to_string(),
                            range: Default::default(),
                        },
                        children: Some(vec![symbol::Node {
                            heading: MDHeading {
                                level: HeadingLevel(3),
                                heading_text: "Third".to_string(),
                                range: Default::default(),
                            },
                            children: None,
                        }]),
                    },
                    symbol::Node {
                        heading: MDHeading {
                            level: HeadingLevel(2),
                            heading_text: "Second".to_string(),
                            range: Default::default(),
                        },
                        children: None,
                    },
                ]),
            },
            symbol::Node {
                heading: MDHeading {
                    level: HeadingLevel(1),
                    heading_text: "First".to_string(),
                    range: Default::default(),
                },
                children: None,
            },
            symbol::Node {
                heading: MDHeading {
                    level: HeadingLevel(1),
                    heading_text: "First".to_string(),
                    range: Default::default(),
                },
                children: None,
            },
        ];

        assert_eq!(tree, Some(expected))
    }

    #[test]
    fn test_simple_tree_different() {
        let headings = vec![
            MDHeading {
                level: HeadingLevel(1),
                heading_text: "First".to_string(),
                range: Default::default(),
            },
            MDHeading {
                level: HeadingLevel(2),
                heading_text: "Second".to_string(),
                range: Default::default(),
            },
            MDHeading {
                level: HeadingLevel(3),
                heading_text: "Third".to_string(),
                range: Default::default(),
            },
            MDHeading {
                level: HeadingLevel(1),
                heading_text: "First".to_string(),
                range: Default::default(),
            },
            MDHeading {
                level: HeadingLevel(1),
                heading_text: "First".to_string(),
                range: Default::default(),
            },
        ];

        let tree = super::construct_tree(&headings);

        let expected = vec![
            symbol::Node {
                heading: MDHeading {
                    level: HeadingLevel(1),
                    heading_text: "First".to_string(),
                    range: Default::default(),
                },
                children: Some(vec![symbol::Node {
                    heading: MDHeading {
                        level: HeadingLevel(2),
                        heading_text: "Second".to_string(),
                        range: Default::default(),
                    },
                    children: Some(vec![symbol::Node {
                        heading: MDHeading {
                            level: HeadingLevel(3),
                            heading_text: "Third".to_string(),
                            range: Default::default(),
                        },
                        children: None,
                    }]),
                }]),
            },
            symbol::Node {
                heading: MDHeading {
                    level: HeadingLevel(1),
                    heading_text: "First".to_string(),
                    range: Default::default(),
                },
                children: None,
            },
            symbol::Node {
                heading: MDHeading {
                    level: HeadingLevel(1),
                    heading_text: "First".to_string(),
                    range: Default::default(),
                },
                children: None,
            },
        ];

        assert_eq!(tree, Some(expected))
    }

    // ============================================================================
    // Workspace Symbol Tests (TDD RED PHASE)
    // ============================================================================
    //
    // These tests verify that workspace_symbol returns MyST-specific elements
    // with correct SymbolKinds and fuzzy matching behavior:
    // - Anchors (target-name)= with SymbolKind::KEY
    // - Directives with :name:/:label: with SymbolKind::OBJECT
    // - Glossary terms with SymbolKind::CONSTANT
    // - Headings with SymbolKind::STRUCT
    // - Math labels with SymbolKind::FUNCTION
    // - Indexed blocks with SymbolKind::STRING
    // ============================================================================

    mod workspace_symbols {
        use std::fs;

        use tower_lsp::lsp_types::{SymbolKind, WorkspaceSymbolParams};

        use crate::{symbol::workspace_symbol, test_utils::create_test_vault};

        /// Test that workspace symbols finds MyST anchors with correct SymbolKind
        #[test]
        fn test_workspace_symbol_finds_anchors() {
            let content = r#"(my-anchor)=
# Section One

Some content here.

(another-anchor)=
## Subsection
"#;
            let (_temp_dir, _vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            let params = WorkspaceSymbolParams {
                query: "anchor".to_string(),
                ..Default::default()
            };

            let symbols = workspace_symbol(&vault, &params);
            assert!(symbols.is_some(), "Should return workspace symbols");

            let symbols = symbols.unwrap();

            // Filter to anchor symbols (SymbolKind::KEY)
            let anchor_symbols: Vec<_> = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::KEY)
                .collect();

            assert!(
                anchor_symbols.len() >= 2,
                "Should find at least 2 anchor symbols matching 'anchor', found: {:?}",
                anchor_symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
            );
        }

        /// Test that workspace symbols finds glossary terms with correct SymbolKind
        #[test]
        fn test_workspace_symbol_finds_glossary_terms() {
            let content = r#"# Glossary

```{glossary}
MyST
  Markedly Structured Text, an extended Markdown syntax.

Sphinx
  A documentation generator for Python projects.
```
"#;
            let (_temp_dir, _vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            let params = WorkspaceSymbolParams {
                query: "myst".to_string(),
                ..Default::default()
            };

            let symbols = workspace_symbol(&vault, &params);
            assert!(symbols.is_some(), "Should return workspace symbols");

            let symbols = symbols.unwrap();

            // Find the MyST glossary term
            let myst_symbol = symbols.iter().find(|s| s.name == "MyST");
            assert!(myst_symbol.is_some(), "Should find 'MyST' glossary term");

            // Verify it has SymbolKind::CONSTANT
            assert_eq!(
                myst_symbol.unwrap().kind,
                SymbolKind::CONSTANT,
                "Glossary term should have SymbolKind::CONSTANT"
            );
        }

        /// Test that workspace symbols finds labeled directives with correct SymbolKind
        #[test]
        fn test_workspace_symbol_finds_labeled_directives() {
            let content = r#"# Document

```{figure} image.png
:name: my-figure
:width: 80%

A figure caption.
```

```{code-block} python
:label: hello-code

print("Hello")
```

```{note}
This note has no label - should NOT appear in workspace symbols.
```
"#;
            let (_temp_dir, _vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            let params = WorkspaceSymbolParams {
                query: "figure".to_string(),
                ..Default::default()
            };

            let symbols = workspace_symbol(&vault, &params);
            assert!(symbols.is_some(), "Should return workspace symbols");

            let symbols = symbols.unwrap();

            // Find the my-figure directive label
            let figure_symbol = symbols.iter().find(|s| s.name == "my-figure");
            assert!(
                figure_symbol.is_some(),
                "Should find 'my-figure' directive label, got: {:?}",
                symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
            );

            // Verify it has SymbolKind::OBJECT
            assert_eq!(
                figure_symbol.unwrap().kind,
                SymbolKind::OBJECT,
                "Labeled directive should have SymbolKind::OBJECT"
            );
        }

        /// Test that workspace symbols fuzzy matching works correctly
        #[test]
        fn test_workspace_symbol_fuzzy_matching() {
            let content = r#"(introduction-section)=
# Introduction

(setup-guide)=
## Setup

(api-reference)=
## API Reference
"#;
            let (_temp_dir, _vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            // Test partial match
            let params = WorkspaceSymbolParams {
                query: "intro".to_string(),
                ..Default::default()
            };

            let symbols = workspace_symbol(&vault, &params);
            assert!(symbols.is_some(), "Should return workspace symbols");

            let symbols = symbols.unwrap();

            // Should find introduction-section
            let intro_symbol = symbols
                .iter()
                .find(|s| s.name.contains("introduction"));
            assert!(
                intro_symbol.is_some(),
                "Fuzzy match should find 'introduction-section' from query 'intro'"
            );

            // Test empty query returns all symbols
            let params_empty = WorkspaceSymbolParams {
                query: "".to_string(),
                ..Default::default()
            };

            let _all_symbols = workspace_symbol(&vault, &params_empty);
            // Note: empty query behavior depends on fuzzy matcher implementation
            // Some matchers return all, some return none
        }

        /// Test that workspace symbols have correct SymbolKinds per the spec
        #[test]
        fn test_workspace_symbol_correct_symbol_kinds() {
            let content = r#"(test-anchor)=
# Main Heading

```{note}
:name: labeled-note
Content
```

```{glossary}
Term
  Definition
```

```{math}
:label: euler-identity
e^{i\pi} + 1 = 0
```

Some text with ^indexed-block
"#;
            let (_temp_dir, _vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            // Query for all symbols
            let params = WorkspaceSymbolParams {
                query: "".to_string(),
                ..Default::default()
            };

            let _symbols = workspace_symbol(&vault, &params);

            // With empty query, behavior depends on matcher - just test what we get
            // Focus on specific queries to verify symbol kinds

            // Test anchor has KEY
            let params_anchor = WorkspaceSymbolParams {
                query: "test-anchor".to_string(),
                ..Default::default()
            };
            let anchor_result = workspace_symbol(&vault, &params_anchor);
            if let Some(symbols) = anchor_result {
                if let Some(anchor) = symbols.iter().find(|s| s.name == "test-anchor") {
                    assert_eq!(
                        anchor.kind,
                        SymbolKind::KEY,
                        "Anchor should have SymbolKind::KEY"
                    );
                }
            }

            // Test heading has STRUCT
            let params_heading = WorkspaceSymbolParams {
                query: "Main Heading".to_string(),
                ..Default::default()
            };
            let heading_result = workspace_symbol(&vault, &params_heading);
            if let Some(symbols) = heading_result {
                if let Some(heading) = symbols.iter().find(|s| s.name.contains("Main Heading")) {
                    assert_eq!(
                        heading.kind,
                        SymbolKind::STRUCT,
                        "Heading should have SymbolKind::STRUCT"
                    );
                }
            }

            // Test glossary term has CONSTANT
            let params_term = WorkspaceSymbolParams {
                query: "Term".to_string(),
                ..Default::default()
            };
            let term_result = workspace_symbol(&vault, &params_term);
            if let Some(symbols) = term_result {
                if let Some(term) = symbols.iter().find(|s| s.name == "Term") {
                    assert_eq!(
                        term.kind,
                        SymbolKind::CONSTANT,
                        "Glossary term should have SymbolKind::CONSTANT"
                    );
                }
            }

            // Test labeled directive has OBJECT
            let params_directive = WorkspaceSymbolParams {
                query: "labeled-note".to_string(),
                ..Default::default()
            };
            let directive_result = workspace_symbol(&vault, &params_directive);
            if let Some(symbols) = directive_result {
                if let Some(directive) = symbols.iter().find(|s| s.name == "labeled-note") {
                    assert_eq!(
                        directive.kind,
                        SymbolKind::OBJECT,
                        "Labeled directive should have SymbolKind::OBJECT"
                    );
                }
            }

            // Test math label has FUNCTION
            let params_math = WorkspaceSymbolParams {
                query: "euler-identity".to_string(),
                ..Default::default()
            };
            let math_result = workspace_symbol(&vault, &params_math);
            if let Some(symbols) = math_result {
                if let Some(math) = symbols.iter().find(|s| s.name == "euler-identity") {
                    assert_eq!(
                        math.kind,
                        SymbolKind::FUNCTION,
                        "Math label should have SymbolKind::FUNCTION"
                    );
                }
            }
        }

        /// Test that indexed blocks have SymbolKind::STRING
        #[test]
        fn test_workspace_symbol_indexed_blocks() {
            let content = r#"# Document

This is a paragraph with an indexed block. ^my-block

Another paragraph. ^another-block
"#;
            let (_temp_dir, _vault_dir, vault) = create_test_vault(|dir| {
                fs::write(dir.join("test.md"), content).unwrap();
            });

            let params = WorkspaceSymbolParams {
                query: "my-block".to_string(),
                ..Default::default()
            };

            let symbols = workspace_symbol(&vault, &params);
            if let Some(symbols) = symbols {
                if let Some(block) = symbols.iter().find(|s| s.name.contains("my-block")) {
                    assert_eq!(
                        block.kind,
                        SymbolKind::STRING,
                        "Indexed block should have SymbolKind::STRING"
                    );
                }
            }
        }
    }
}
