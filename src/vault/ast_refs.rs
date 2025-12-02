//! AST-based reference extraction using markdown-rs.
//!
//! This module provides an alternative to regex-based parsing,
//! extracting references by traversing the markdown AST.
//!
//! # Architecture
//!
//! The extraction works by parsing CommonMark nodes: `Link`, `FootnoteReference`,
//! `LinkReference` are directly parsed by markdown-rs and extracted from the AST.
//! MyST roles are extracted via regex on text nodes.

use markdown::{mdast::Node, to_mdast, ParseOptions};
use once_cell::sync::Lazy;
use regex::Regex;
use ropey::Rope;

use super::{MyRange, MystRoleKind, Reference, ReferenceData};

/// Extract all references from markdown text using AST parsing.
///
/// This is the AST-based replacement for `Reference::new()`.
///
/// # Arguments
/// * `text` - The markdown source text
/// * `file_name` - The name of the current file (used for same-file references like `[[#heading]]`)
///
/// # Returns
/// An iterator over all `Reference` items found in the document.
pub fn extract_references_from_ast(text: &str, file_name: &str) -> Vec<Reference> {
    // Parse with GFM options to get footnote support
    let parse_opts = ParseOptions::gfm();

    let ast = match to_mdast(text, &parse_opts) {
        Ok(node) => node,
        Err(_) => return Vec::new(),
    };

    let rope = Rope::from_str(text);
    let mut refs = Vec::new();

    // Check if document has any link reference definitions (for LinkRef extraction)
    let has_definitions = has_link_definitions(&ast);

    traverse_node(&ast, text, file_name, &rope, has_definitions, &mut refs);

    refs
}

/// Check if the AST contains any Definition nodes (link reference definitions).
fn has_link_definitions(node: &Node) -> bool {
    match node {
        Node::Definition(_) => true,
        _ => {
            if let Some(children) = node.children() {
                children.iter().any(has_link_definitions)
            } else {
                false
            }
        }
    }
}

/// Recursively traverse AST nodes and extract references.
fn traverse_node(
    node: &Node,
    text: &str,
    _file_name: &str, // Kept for API compatibility, passed through recursion
    rope: &Rope,
    has_definitions: bool,
    refs: &mut Vec<Reference>,
) {
    match node {
        Node::Link(link) => {
            if let Some(reference) = extract_md_link(link, text, rope) {
                refs.push(reference);
            }
        }
        Node::Image(image) => {
            if let Some(reference) = extract_image_link(image, rope) {
                refs.push(reference);
            }
        }
        Node::Text(text_node) => {
            // Extract footnotes from text nodes (when no definition exists,
            // markdown-rs doesn't parse them as FootnoteReference)
            let footnotes = extract_footnotes_from_text(text_node, rope);
            refs.extend(footnotes);

            // Extract MyST substitutions: {{variable_name}}
            let substitutions = extract_substitutions_from_text(text_node, rope);
            refs.extend(substitutions);
        }
        Node::FootnoteReference(fref) => {
            if let Some(reference) = extract_footnote_ref(fref, rope) {
                refs.push(reference);
            }
        }
        Node::LinkReference(lref) => {
            // Only extract link references if definitions exist in the document
            if has_definitions {
                if let Some(reference) = extract_link_ref(lref, rope) {
                    refs.push(reference);
                }
            }
        }
        Node::Paragraph(para) => {
            // Extract MyST roles from paragraph children
            // MyST roles like {ref}`target` are parsed as Text + InlineCode sibling pairs
            let roles = extract_myst_roles_from_siblings(&para.children, rope);
            refs.extend(roles);
        }
        _ => {}
    }

    // Recurse into children
    if let Some(children) = node.children() {
        for child in children {
            traverse_node(child, text, _file_name, rope, has_definitions, refs);
        }
    }
}

/// Extract a Reference from a markdown Link node.
///
/// Handles:
/// - `[display](file.md)` -> MDFileLink
/// - `[display](file.md#heading)` -> MDHeadingLink
/// - `[display](file.md#^block)` -> MDIndexedBlockLink
///
/// Skips external URLs (http://, https://, data:).
fn extract_md_link(link: &markdown::mdast::Link, _text: &str, rope: &Rope) -> Option<Reference> {
    let url = &link.url;

    // Skip external URLs
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("data:") {
        return None;
    }

    // Get position for range calculation
    let range = MyRange::from_ast_position(link.position.as_ref(), rope)?;

    // URL decode the path
    let decoded_url = urlencoding::decode(url)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| url.clone());

    // Parse path and fragment
    let (path, fragment) = if let Some(hash_pos) = decoded_url.find('#') {
        let (p, f) = decoded_url.split_at(hash_pos);
        (p.to_string(), Some(&f[1..])) // Skip the '#'
    } else {
        (decoded_url, None)
    };

    // Strip .md extension if present for reference_text
    let path_without_ext = path
        .strip_suffix(".md")
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.clone());

    // Only accept .md files or files without extension
    // Check only the filename portion for extensions, not the whole path
    // (e.g., "./path/to/file" should work, but "./path/to/file.txt" should not)
    if !path.is_empty() && !path.ends_with(".md") {
        // Get the filename portion (after the last slash)
        let filename = path.rsplit('/').next().unwrap_or(&path);
        if filename.contains('.') {
            return None;
        }
    }

    // Extract display text from link children
    let display_text = extract_text_from_children(&link.children);

    match fragment {
        Some(frag) if frag.starts_with('^') => {
            // Indexed block link
            let index = &frag[1..]; // Skip the '^'
            Some(Reference::MDIndexedBlockLink(
                ReferenceData {
                    reference_text: format!("{}#{}", path_without_ext, frag),
                    display_text,
                    range,
                },
                path_without_ext,
                index.to_string(),
            ))
        }
        Some(heading) => {
            // Heading link
            Some(Reference::MDHeadingLink(
                ReferenceData {
                    reference_text: format!("{}#{}", path_without_ext, heading),
                    display_text,
                    range,
                },
                path_without_ext,
                heading.to_string(),
            ))
        }
        None => {
            // Plain file link
            Some(Reference::MDFileLink(ReferenceData {
                reference_text: path_without_ext,
                display_text,
                range,
            }))
        }
    }
}

/// Extract a Reference from a markdown Image node.
///
/// Handles: `![alt](path)` -> ImageLink
///
/// Skips external URLs (http://, https://, data:).
fn extract_image_link(image: &markdown::mdast::Image, rope: &Rope) -> Option<Reference> {
    let url = &image.url;

    // Skip external URLs
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("data:") {
        return None;
    }

    // Get position for range calculation
    let range = MyRange::from_ast_position(image.position.as_ref(), rope)?;

    // URL decode the path
    let decoded_url = urlencoding::decode(url)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| url.clone());

    // Get alt text as display text
    let display_text = if image.alt.is_empty() {
        None
    } else {
        Some(image.alt.clone())
    };

    Some(Reference::ImageLink(ReferenceData {
        reference_text: decoded_url,
        display_text,
        range,
    }))
}

/// Extract text content from a list of child nodes.
fn extract_text_from_children(children: &[Node]) -> Option<String> {
    let text: String = children
        .iter()
        .filter_map(|child| match child {
            Node::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect();

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Regex for parsing footnote references from text nodes.
///
/// This handles footnotes that appear in text when there's no definition
/// (markdown-rs only parses FootnoteReference when definition exists).
///
/// Matches: [^identifier] but NOT [^identifier]: (which is a definition)
static FOOTNOTE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?<start>\[?)(?<full>\[(?<index>\^[^\[\] ]+)\])(?<end>:?)").unwrap());

/// Extract footnote references from a Text node.
///
/// Footnotes [^ref] that don't have definitions are not parsed as
/// FootnoteReference by markdown-rs, so we use regex extraction.
fn extract_footnotes_from_text(text_node: &markdown::mdast::Text, rope: &Rope) -> Vec<Reference> {
    let position = match &text_node.position {
        Some(pos) => pos,
        None => return Vec::new(),
    };

    let base_offset = position.start.offset;
    let node_text = &text_node.value;

    FOOTNOTE_RE
        .captures_iter(node_text)
        // Filter out definitions (start with [ and end with :) and nested brackets
        .filter(|capture| {
            matches!(
                (capture.name("start"), capture.name("end")),
                (Some(start), Some(end)) if !start.as_str().starts_with('[') && !end.as_str().ends_with(':')
            )
        })
        .filter_map(|capture| {
            let full = capture.name("full")?;
            let index = capture.name("index")?;

            let start = base_offset + full.start();
            let end = base_offset + full.end();
            let range = MyRange::from_range(rope, start..end);

            Some(Reference::Footnote(ReferenceData {
                reference_text: index.as_str().to_string(),
                display_text: None,
                range,
            }))
        })
        .collect()
}

/// Extract a Reference from a FootnoteReference node.
fn extract_footnote_ref(
    fref: &markdown::mdast::FootnoteReference,
    rope: &Rope,
) -> Option<Reference> {
    let range = MyRange::from_ast_position(fref.position.as_ref(), rope)?;

    Some(Reference::Footnote(ReferenceData {
        reference_text: format!("^{}", fref.identifier),
        display_text: None,
        range,
    }))
}

/// Extract a Reference from a LinkReference node.
fn extract_link_ref(lref: &markdown::mdast::LinkReference, rope: &Rope) -> Option<Reference> {
    let range = MyRange::from_ast_position(lref.position.as_ref(), rope)?;

    Some(Reference::LinkRef(ReferenceData {
        reference_text: lref.identifier.clone(),
        display_text: None,
        range,
    }))
}

// ============================================================================
// MyST Role Extraction
// ============================================================================

/// Regex for MyST substitutions: {{variable_name}}
///
/// Matches `{{name}}` where name starts with a letter or underscore,
/// followed by alphanumeric characters or underscores.
static SUBSTITUTION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\{(?<name>[a-zA-Z_][a-zA-Z0-9_]*)\}\}").unwrap());

/// Regex for detecting Text nodes that end with a MyST role prefix: `{rolename}`
///
/// markdown-rs parses `{ref}`target`` as:
///   Text("...{ref}") + InlineCode("target")
/// So we need to detect this pattern across sibling nodes.
static MYST_ROLE_PREFIX_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(?<role>[a-zA-Z][a-zA-Z0-9_-]*)\}$").unwrap());

/// Regex for parsing display text with target format: "display text <target>"
static ROLE_TARGET_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?<display>.+?)\s*<(?<target>[^>]+)>$").unwrap());

/// Extract substitutions from a Text node.
///
/// MyST substitutions use `{{variable}}` syntax. This function extracts
/// them from text nodes (NOT InlineCode or Code nodes, which are handled
/// by the AST traversal that skips those node types).
fn extract_substitutions_from_text(
    text_node: &markdown::mdast::Text,
    rope: &Rope,
) -> Vec<Reference> {
    let position = match &text_node.position {
        Some(pos) => pos,
        None => return Vec::new(),
    };

    let base_offset = position.start.offset;
    let node_text = &text_node.value;

    SUBSTITUTION_RE
        .captures_iter(node_text)
        .filter_map(|capture| {
            let full_match = capture.get(0)?;
            let name = capture.name("name")?;

            let start = base_offset + full_match.start();
            let end = base_offset + full_match.end();
            let range = MyRange::from_range(rope, start..end);

            Some(Reference::Substitution(ReferenceData {
                reference_text: name.as_str().to_string(),
                display_text: None,
                range,
            }))
        })
        .collect()
}

/// Parse role kind from role name string.
fn parse_role_kind(role_name: &str) -> Option<MystRoleKind> {
    match role_name {
        "ref" => Some(MystRoleKind::Ref),
        "numref" => Some(MystRoleKind::NumRef),
        "eq" => Some(MystRoleKind::Eq),
        "doc" => Some(MystRoleKind::Doc),
        "download" => Some(MystRoleKind::Download),
        "term" => Some(MystRoleKind::Term),
        "abbr" => Some(MystRoleKind::Abbr),
        _ => None, // Unknown role
    }
}

/// Extract MyST role references from paragraph children.
///
/// MyST roles like `{ref}`target`` are parsed by markdown-rs as:
///   Text("{ref}") + InlineCode("target")
///
/// We scan sibling nodes to detect this pattern:
/// - Text ending with `{rolename}` followed by InlineCode
fn extract_myst_roles_from_siblings(children: &[Node], rope: &Rope) -> Vec<Reference> {
    let mut roles = Vec::new();

    // Look at pairs: Text + InlineCode
    for window in children.windows(2) {
        if let [Node::Text(text_node), Node::InlineCode(code_node)] = window {
            // Check if text ends with {rolename}
            if let Some(caps) = MYST_ROLE_PREFIX_RE.captures(&text_node.value) {
                let role_name = caps.name("role").map(|m| m.as_str()).unwrap_or("");

                if let Some(role_kind) = parse_role_kind(role_name) {
                    let content = &code_node.value;

                    // Parse content for optional display text: "display <target>" or just "target"
                    let (target, display_text) =
                        if let Some(target_caps) = ROLE_TARGET_RE.captures(content) {
                            let display = target_caps
                                .name("display")
                                .map(|m| m.as_str().trim().to_string());
                            let target = target_caps
                                .name("target")
                                .map(|m| m.as_str().to_string())
                                .unwrap_or_else(|| content.to_string());
                            (target, display)
                        } else {
                            (content.to_string(), None)
                        };

                    // Calculate range: from {role} start to InlineCode end
                    // The role prefix is at the end of text_node
                    let role_match = caps.get(0).unwrap();
                    let text_pos = text_node.position.as_ref();
                    let code_pos = code_node.position.as_ref();

                    if let (Some(text_pos), Some(code_pos)) = (text_pos, code_pos) {
                        // Start: text node start offset + where {role} begins in text
                        let start = text_pos.start.offset + role_match.start();
                        // End: InlineCode end (includes closing backtick)
                        let end = code_pos.end.offset;
                        let range = MyRange::from_range(rope, start..end);

                        roles.push(Reference::MystRole(
                            ReferenceData {
                                reference_text: target.clone(),
                                display_text,
                                range,
                            },
                            role_kind,
                            target,
                        ));
                    }
                }
            }
        }
    }

    roles
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Step 2.1: MD Link Tests
    // ========================================================================

    #[test]
    fn test_md_link_simple() {
        let text = "[click here](other.md)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        assert!(matches!(&refs[0], Reference::MDFileLink(_)));
        assert_eq!(refs[0].data().reference_text, "other");
        assert_eq!(refs[0].data().display_text, Some("click here".to_string()));
    }

    #[test]
    fn test_md_link_with_heading() {
        let text = "[go to section](doc.md#introduction)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        match &refs[0] {
            Reference::MDHeadingLink(data, file, heading) => {
                assert_eq!(data.reference_text, "doc#introduction");
                assert_eq!(file, "doc");
                assert_eq!(heading, "introduction");
            }
            _ => panic!("Expected MDHeadingLink"),
        }
    }

    #[test]
    fn test_md_link_with_block() {
        let text = "[see block](notes.md#^abc123)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        match &refs[0] {
            Reference::MDIndexedBlockLink(data, file, index) => {
                assert_eq!(data.reference_text, "notes#^abc123");
                assert_eq!(file, "notes");
                assert_eq!(index, "abc123");
            }
            _ => panic!("Expected MDIndexedBlockLink"),
        }
    }

    #[test]
    fn test_md_link_with_block_no_extension() {
        // Like path/to/link#^index1 (no .md extension)
        let text = "[link](path/to/link#^index1)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1, "Should extract 1 reference");
        match &refs[0] {
            Reference::MDIndexedBlockLink(data, file, index) => {
                assert_eq!(data.reference_text, "path/to/link#^index1");
                assert_eq!(file, "path/to/link");
                assert_eq!(index, "index1");
            }
            _ => panic!("Expected MDIndexedBlockLink, got {:?}", refs[0]),
        }
    }

    #[test]
    fn test_md_link_with_trailing_colon() {
        // Regression test: link followed by colon on multiline text
        // Note: 4+ spaces of indentation creates a code block in markdown!
        // The original test text was indented with 12 spaces, making it a code block.
        let text = r#"
Buggy cross [link](path/to/link#^index1):

(this causes bug)
"#;
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        eprintln!("Refs found: {:?}", refs);
        assert_eq!(refs.len(), 1, "Should extract 1 reference");
        match &refs[0] {
            Reference::MDIndexedBlockLink(data, file, index) => {
                assert_eq!(data.reference_text, "path/to/link#^index1");
                assert_eq!(file, "path/to/link");
                assert_eq!(index, "index1");
            }
            _ => panic!("Expected MDIndexedBlockLink, got {:?}", refs[0]),
        }
    }

    #[test]
    fn test_md_link_skip_external_http() {
        let text = "[example](https://example.com)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");
        assert!(refs.is_empty(), "Should skip https URLs");
    }

    #[test]
    fn test_md_link_skip_external_http_plain() {
        let text = "[example](http://example.com)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");
        assert!(refs.is_empty(), "Should skip http URLs");
    }

    #[test]
    fn test_md_link_skip_data_uri() {
        let text = "[image](data:image/png;base64,abc)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");
        assert!(refs.is_empty(), "Should skip data URIs");
    }

    #[test]
    fn test_md_link_url_encoded() {
        let text = "[doc](my%20file.md)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].data().reference_text, "my file");
    }

    #[test]
    fn test_md_link_relative_path() {
        let text = "[doc](./subdir/file.md)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].data().reference_text, "./subdir/file");
    }

    #[test]
    fn test_md_link_no_extension() {
        let text = "[doc](readme)";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].data().reference_text, "readme");
    }

    // ========================================================================
    // Step 2.3: Footnote Tests
    // ========================================================================

    #[test]
    fn test_footnote_reference() {
        let text = "Some text[^note] more text.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        match &refs[0] {
            Reference::Footnote(data) => {
                assert_eq!(data.reference_text, "^note");
            }
            _ => panic!("Expected Footnote"),
        }
    }

    #[test]
    fn test_footnote_definition_not_extracted() {
        // Footnote definitions should NOT be extracted as references
        let text = "[^note]: This is the footnote content.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        // Should be empty - definitions are not references
        assert!(
            refs.is_empty(),
            "Footnote definitions should not be extracted as references"
        );
    }

    #[test]
    fn test_multiple_footnotes() {
        let text = "First[^a] and second[^b] footnotes.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 2);
        assert!(matches!(&refs[0], Reference::Footnote(_)));
        assert!(matches!(&refs[1], Reference::Footnote(_)));
    }

    // ========================================================================
    // Step 2.4: Link Reference Tests
    // ========================================================================

    #[test]
    fn test_link_ref_with_definition() {
        let text = "Use [example] in text.\n\n[example]: http://example.com";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        match &refs[0] {
            Reference::LinkRef(data) => {
                assert_eq!(data.reference_text, "example");
            }
            _ => panic!("Expected LinkRef"),
        }
    }

    #[test]
    fn test_link_ref_without_definition() {
        // Without a definition, [ref] is just text, not a link reference
        let text = "Use [example] in text.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        // Should be empty - no definition means no LinkRef
        assert!(
            refs.is_empty(),
            "Link references without definitions should not be extracted"
        );
    }

    #[test]
    fn test_multiple_link_refs() {
        let text = "See [foo] and [bar] here.\n\n[foo]: http://foo.com\n[bar]: http://bar.com";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 2);
        assert!(refs.iter().all(|r| matches!(r, Reference::LinkRef(_))));
    }

    // ========================================================================
    // Integration Tests
    // ========================================================================

    #[test]
    fn test_mixed_references() {
        // Note: Link definitions must be at root level, not inside footnote definitions
        let text = r#"# Document

Check [md link](other.md) and [another](doc.md#heading).

See footnote[^1] and reference [ref].

[ref]: http://example.com

[^1]: Footnote text
"#;
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        // Should have: 2 md links, 1 footnote, 1 link ref = 4
        assert_eq!(refs.len(), 4);

        let md_count = refs
            .iter()
            .filter(|r| {
                matches!(
                    r,
                    Reference::MDFileLink(_) | Reference::MDHeadingLink(_, _, _)
                )
            })
            .count();
        let footnote_count = refs
            .iter()
            .filter(|r| matches!(r, Reference::Footnote(_)))
            .count();
        let linkref_count = refs
            .iter()
            .filter(|r| matches!(r, Reference::LinkRef(_)))
            .count();

        assert_eq!(md_count, 2);
        assert_eq!(footnote_count, 1);
        assert_eq!(linkref_count, 1);
    }

    #[test]
    fn test_range_positions() {
        let text = "Start [link](file.md) end";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        let range = &refs[0].data().range;

        // [link](file.md) starts at position 6 and ends at 21
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 21);
    }

    #[test]
    fn test_multiline_range() {
        let text = "First line\n[link](file.md) on second line";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        let range = &refs[0].data().range;

        // [link](file.md) is on line 1 (0-indexed), starting at character 0
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
    }

    #[test]
    fn test_empty_document() {
        let refs: Vec<_> = extract_references_from_ast("", "test");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_document_with_no_references() {
        let text = "# Just a heading\n\nSome plain text without any links.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");
        assert!(refs.is_empty());
    }

    // ========================================================================
    // MyST Role Extraction Tests
    // ========================================================================

    #[test]
    fn test_myst_ref_role_extraction() {
        let text = "See {ref}`my-section` for details.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        match &refs[0] {
            Reference::MystRole(data, kind, target) => {
                assert_eq!(*kind, MystRoleKind::Ref);
                assert_eq!(target, "my-section");
                assert_eq!(data.reference_text, "my-section");
                assert!(data.display_text.is_none());
            }
            _ => panic!("Expected MystRole, got {:?}", refs[0]),
        }
    }

    #[test]
    fn test_myst_doc_role_extraction() {
        let text = "Read {doc}`./other-file` next.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        match &refs[0] {
            Reference::MystRole(data, kind, target) => {
                assert_eq!(*kind, MystRoleKind::Doc);
                assert_eq!(target, "./other-file");
                assert_eq!(data.reference_text, "./other-file");
            }
            _ => panic!("Expected MystRole, got {:?}", refs[0]),
        }
    }

    #[test]
    fn test_myst_role_with_display_text() {
        let text = "See {ref}`the section <my-target>` here.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        match &refs[0] {
            Reference::MystRole(data, kind, target) => {
                assert_eq!(*kind, MystRoleKind::Ref);
                assert_eq!(target, "my-target");
                assert_eq!(data.reference_text, "my-target");
                assert_eq!(data.display_text, Some("the section".to_string()));
            }
            _ => panic!("Expected MystRole, got {:?}", refs[0]),
        }
    }

    #[test]
    fn test_myst_term_role_extraction() {
        let text = "The {term}`dialectical materialism` is important.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        match &refs[0] {
            Reference::MystRole(_, kind, target) => {
                assert_eq!(*kind, MystRoleKind::Term);
                assert_eq!(target, "dialectical materialism");
            }
            _ => panic!("Expected MystRole"),
        }
    }

    #[test]
    fn test_myst_multiple_roles() {
        let text = "See {ref}`section-a` and {doc}`./file-b` for more info.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 2);

        let role_kinds: Vec<_> = refs
            .iter()
            .filter_map(|r| match r {
                Reference::MystRole(_, kind, _) => Some(*kind),
                _ => None,
            })
            .collect();

        assert!(role_kinds.contains(&MystRoleKind::Ref));
        assert!(role_kinds.contains(&MystRoleKind::Doc));
    }

    #[test]
    fn test_myst_all_role_types() {
        let text = r#"
{ref}`target1`
{numref}`Figure %s <fig-label>`
{eq}`equation1`
{doc}`/path/to/doc`
{download}`file.zip`
{term}`glossary-term`
{abbr}`MyST (Markedly Structured Text)`
"#;
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 7);

        let kinds: Vec<_> = refs
            .iter()
            .filter_map(|r| match r {
                Reference::MystRole(_, kind, _) => Some(*kind),
                _ => None,
            })
            .collect();

        assert!(kinds.contains(&MystRoleKind::Ref));
        assert!(kinds.contains(&MystRoleKind::NumRef));
        assert!(kinds.contains(&MystRoleKind::Eq));
        assert!(kinds.contains(&MystRoleKind::Doc));
        assert!(kinds.contains(&MystRoleKind::Download));
        assert!(kinds.contains(&MystRoleKind::Term));
        assert!(kinds.contains(&MystRoleKind::Abbr));
    }

    #[test]
    fn test_myst_unknown_role_ignored() {
        let text = "Using {unknown}`something` role.";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        // Unknown roles should be ignored
        assert!(refs.is_empty());
    }

    #[test]
    fn test_myst_role_range_position() {
        let text = "Start {ref}`my-target` end";
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        assert_eq!(refs.len(), 1);
        let range = &refs[0].data().range;

        // {ref}`my-target` starts at position 6 and ends at 22
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 22);
    }

    #[test]
    fn test_myst_mixed_with_other_references() {
        let text = r#"# Document

Check [md link](other.md) and {ref}`my-section`.

See footnote[^1] too.

[^1]: Footnote text
"#;
        let refs: Vec<_> = extract_references_from_ast(text, "test");

        let md_count = refs
            .iter()
            .filter(|r| matches!(r, Reference::MDFileLink(_)))
            .count();
        let role_count = refs
            .iter()
            .filter(|r| matches!(r, Reference::MystRole(..)))
            .count();
        let footnote_count = refs
            .iter()
            .filter(|r| matches!(r, Reference::Footnote(_)))
            .count();

        assert_eq!(md_count, 1);
        assert_eq!(role_count, 1);
        assert_eq!(footnote_count, 1);
    }

    // ========================================================================
    // Image Link Extraction Tests
    // ========================================================================

    #[test]
    fn test_image_link_extraction() {
        let text = "![Example](./images/photo.png)";
        let refs: Vec<_> = extract_references_from_ast(text, "test.md");

        assert_eq!(refs.len(), 1);
        assert!(
            matches!(&refs[0], Reference::ImageLink(_)),
            "Expected ImageLink, got {:?}",
            refs[0]
        );
        assert_eq!(refs[0].data().reference_text, "./images/photo.png");
    }

    #[test]
    fn test_image_skip_external_https() {
        let text = "![Logo](https://example.com/logo.png)";
        let refs: Vec<_> = extract_references_from_ast(text, "test.md");

        assert!(refs.is_empty(), "External https images should be skipped");
    }

    #[test]
    fn test_image_skip_external_http() {
        let text = "![Logo](http://example.com/logo.png)";
        let refs: Vec<_> = extract_references_from_ast(text, "test.md");

        assert!(refs.is_empty(), "External http images should be skipped");
    }

    #[test]
    fn test_image_skip_data_uri() {
        let text = "![Inline](data:image/png;base64,ABC123)";
        let refs: Vec<_> = extract_references_from_ast(text, "test.md");

        assert!(refs.is_empty(), "Data URIs should be skipped");
    }

    #[test]
    fn test_image_with_title() {
        let text = r#"![Alt text](./image.png "Title")"#;
        let refs: Vec<_> = extract_references_from_ast(text, "test.md");

        assert_eq!(refs.len(), 1);
        assert!(
            matches!(&refs[0], Reference::ImageLink(_)),
            "Expected ImageLink, got {:?}",
            refs[0]
        );
    }

    #[test]
    fn test_image_with_alt_text() {
        let text = "![My alt text](assets/diagram.svg)";
        let refs: Vec<_> = extract_references_from_ast(text, "test.md");

        assert_eq!(refs.len(), 1);
        match &refs[0] {
            Reference::ImageLink(data) => {
                assert_eq!(data.reference_text, "assets/diagram.svg");
                assert_eq!(data.display_text, Some("My alt text".to_string()));
            }
            _ => panic!("Expected ImageLink"),
        }
    }

    #[test]
    fn test_multiple_images() {
        let text = "![First](a.png) and ![Second](b.jpg)";
        let refs: Vec<_> = extract_references_from_ast(text, "test.md");

        assert_eq!(refs.len(), 2);
        assert!(refs.iter().all(|r| matches!(r, Reference::ImageLink(_))));
    }

    #[test]
    fn test_image_range_position() {
        let text = "Start ![img](photo.png) end";
        let refs: Vec<_> = extract_references_from_ast(text, "test.md");

        assert_eq!(refs.len(), 1);
        let range = &refs[0].data().range;

        // ![img](photo.png) starts at position 6 and ends at 23
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 23);
    }

    #[test]
    fn test_mixed_images_and_links() {
        let text = "See [link](doc.md) and ![image](photo.png)";
        let refs: Vec<_> = extract_references_from_ast(text, "test.md");

        assert_eq!(refs.len(), 2);

        let link_count = refs
            .iter()
            .filter(|r| matches!(r, Reference::MDFileLink(_)))
            .count();
        let image_count = refs
            .iter()
            .filter(|r| matches!(r, Reference::ImageLink(_)))
            .count();

        assert_eq!(link_count, 1);
        assert_eq!(image_count, 1);
    }

    // ========================================================================
    // MyST Substitution Extraction Tests (Chunk 8)
    // ========================================================================

    #[test]
    fn test_substitution_extraction() {
        let text = "The {{project_name}} is ready.";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].data().reference_text, "project_name");
    }

    #[test]
    fn test_substitution_in_code_block_ignored() {
        let text = "```\n{{not_a_sub}}\n```";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert!(
            subs.is_empty(),
            "Substitutions inside code blocks should be ignored"
        );
    }

    #[test]
    fn test_substitution_in_fenced_code_block_ignored() {
        let text = "```python\nprint('{{template}}')\n```";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert!(
            subs.is_empty(),
            "Substitutions inside fenced code blocks should be ignored"
        );
    }

    #[test]
    fn test_multiple_substitutions() {
        let text = "{{one}} and {{two}} and {{three}}";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert_eq!(subs.len(), 3);

        let names: Vec<_> = subs
            .iter()
            .map(|s| s.data().reference_text.as_str())
            .collect();
        assert!(names.contains(&"one"));
        assert!(names.contains(&"two"));
        assert!(names.contains(&"three"));
    }

    #[test]
    fn test_substitution_with_underscores() {
        let text = "The {{my_long_variable_name}} value.";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].data().reference_text, "my_long_variable_name");
    }

    #[test]
    fn test_substitution_with_numbers() {
        let text = "Version {{version_2}} is out.";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].data().reference_text, "version_2");
    }

    #[test]
    fn test_substitution_invalid_starting_with_number() {
        // Substitution names must start with a letter or underscore
        let text = "Invalid {{123abc}} here.";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert!(
            subs.is_empty(),
            "Names starting with numbers should be invalid"
        );
    }

    #[test]
    fn test_substitution_range_position() {
        let text = "Start {{myvar}} end";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert_eq!(subs.len(), 1);
        let range = &subs[0].data().range;

        // {{myvar}} starts at position 6 and ends at 15
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 15);
    }

    #[test]
    fn test_substitution_multiline() {
        let text = "First line\n{{var_on_line_two}} on second line";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert_eq!(subs.len(), 1);
        let range = &subs[0].data().range;

        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
    }

    #[test]
    fn test_substitution_in_inline_code_ignored() {
        let text = "Use `{{template}}` in your config.";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert!(
            subs.is_empty(),
            "Substitutions inside inline code should be ignored"
        );
    }

    #[test]
    fn test_substitution_mixed_with_other_references() {
        let text = "The {{project}} version. See [link](doc.md) for more.";
        let refs = extract_references_from_ast(text, "test.md");

        let sub_count = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .count();
        let link_count = refs
            .iter()
            .filter(|r| matches!(r, Reference::MDFileLink(_)))
            .count();

        assert_eq!(sub_count, 1);
        assert_eq!(link_count, 1);
    }

    #[test]
    fn test_empty_substitution_ignored() {
        // Empty braces should not be extracted
        let text = "Empty {{}} should be ignored.";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert!(subs.is_empty(), "Empty substitutions should be ignored");
    }

    #[test]
    fn test_substitution_adjacent_to_text() {
        let text = "Hello{{name}}World";
        let refs = extract_references_from_ast(text, "test.md");
        let subs: Vec<_> = refs
            .iter()
            .filter(|r| matches!(r, Reference::Substitution(_)))
            .collect();

        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].data().reference_text, "name");
    }
}
