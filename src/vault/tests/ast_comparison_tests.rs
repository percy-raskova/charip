//! Comparison tests between AST-based and regex-based reference extraction.
//!
//! These tests verify that the new AST implementation produces equivalent
//! results to the existing regex implementation for reference extraction.

use crate::vault::ast_refs::extract_references_from_ast;
use crate::vault::Reference;

/// Helper to sort references by position for comparison.
fn sort_refs(refs: &mut [Reference]) {
    refs.sort_by(|a, b| {
        let a_range = &a.data().range;
        let b_range = &b.data().range;
        a_range
            .start
            .line
            .cmp(&b_range.start.line)
            .then(a_range.start.character.cmp(&b_range.start.character))
    });
}

/// Filter out tags from references (AST impl doesn't extract tags).
fn filter_tags(refs: Vec<Reference>) -> Vec<Reference> {
    refs.into_iter()
        .filter(|r| !matches!(r, Reference::Tag(_)))
        .collect()
}

/// Compare two reference lists, checking type, reference_text, display_text, and range.
fn refs_equal(a: &Reference, b: &Reference) -> bool {
    // First check that they're the same type
    if !a.matches_type(b) {
        return false;
    }

    let a_data = a.data();
    let b_data = b.data();

    // Compare core data
    a_data.reference_text == b_data.reference_text
        && a_data.display_text == b_data.display_text
        && a_data.range == b_data.range
}

/// Detailed comparison with diagnostic output.
fn compare_refs(regex_refs: &[Reference], ast_refs: &[Reference]) -> bool {
    if regex_refs.len() != ast_refs.len() {
        eprintln!(
            "Length mismatch: regex={}, ast={}",
            regex_refs.len(),
            ast_refs.len()
        );
        eprintln!("Regex refs: {:?}", regex_refs);
        eprintln!("AST refs: {:?}", ast_refs);
        return false;
    }

    for (i, (r, a)) in regex_refs.iter().zip(ast_refs.iter()).enumerate() {
        if !refs_equal(r, a) {
            eprintln!("Mismatch at index {}", i);
            eprintln!("  Regex: {:?}", r);
            eprintln!("  AST:   {:?}", a);
            return false;
        }
    }
    true
}

// ============================================================================
// MD Link Tests
// ============================================================================

#[test]
fn test_ast_matches_regex_simple_md_link() {
    let text = "[click here](other.md)";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "Simple MD link should match"
    );
}

#[test]
fn test_ast_matches_regex_md_link_with_heading() {
    let text = "[go to section](doc.md#introduction)";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "MD link with heading should match"
    );
}

#[test]
fn test_ast_matches_regex_md_link_with_block() {
    let text = "[see block](notes.md#^abc123)";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "MD link with block ref should match"
    );
}

#[test]
fn test_ast_matches_regex_md_link_no_extension() {
    let text = "[doc](readme)";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "MD link without extension should match"
    );
}

#[test]
fn test_ast_matches_regex_md_link_relative_path() {
    let text = "[doc](./subdir/file.md)";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "MD link with relative path should match"
    );
}

// ============================================================================
// Footnote Tests
// ============================================================================

#[test]
fn test_ast_matches_regex_footnote() {
    let text = "Some text[^note] more text.";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "Footnote reference should match"
    );
}

#[test]
fn test_ast_matches_regex_multiple_footnotes() {
    let text = "First[^a] and second[^b] footnotes.";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "Multiple footnotes should match"
    );
}

// ============================================================================
// Link Reference Tests
// ============================================================================

#[test]
fn test_ast_matches_regex_link_ref_with_definition() {
    let text = "Use [example] in text.\n\n[example]: http://example.com";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "Link reference with definition should match"
    );
}

#[test]
fn test_ast_matches_regex_no_link_ref_without_definition() {
    // Without a definition, neither implementation should extract a LinkRef
    let text = "Use [example] in text.";
    let file_name = "test";

    let regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    assert!(
        regex_refs.is_empty(),
        "Regex should not extract LinkRef without definition"
    );
    assert!(
        ast_refs.is_empty(),
        "AST should not extract LinkRef without definition"
    );
}

// ============================================================================
// External URLs (should be skipped)
// ============================================================================

#[test]
fn test_ast_matches_regex_skip_https() {
    let text = "[example](https://example.com)";
    let file_name = "test";

    let regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    assert!(regex_refs.is_empty(), "Regex should skip https URLs");
    assert!(ast_refs.is_empty(), "AST should skip https URLs");
}

#[test]
fn test_ast_matches_regex_skip_http() {
    let text = "[example](http://example.com)";
    let file_name = "test";

    let regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    assert!(regex_refs.is_empty(), "Regex should skip http URLs");
    assert!(ast_refs.is_empty(), "AST should skip http URLs");
}

// ============================================================================
// Mixed Content Tests
// ============================================================================

#[test]
fn test_ast_matches_regex_complex_document() {
    let text = r#"# Document

Check [md link](other.md) and [another](doc.md#section).

See footnote[^1] and reference [ref].

[ref]: http://example.com

[^1]: Footnote text
"#;
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "Complex document with mixed references should match"
    );
}

#[test]
fn test_ast_matches_regex_multiline_content() {
    let text = "First line\n[link](other.md) on second line\n[md](file.md) on third";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "Multiline content should match"
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_ast_matches_regex_empty_document() {
    let text = "";
    let file_name = "test";

    let regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    assert!(regex_refs.is_empty());
    assert!(ast_refs.is_empty());
}

#[test]
fn test_ast_matches_regex_no_references() {
    let text = "Just plain text with no links or references.";
    let file_name = "test";

    let regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    assert!(regex_refs.is_empty());
    assert!(ast_refs.is_empty());
}

#[test]
fn test_ast_matches_regex_url_encoded() {
    let text = "[doc](my%20file.md)";
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = extract_references_from_ast(text, file_name);

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "URL encoded link should match"
    );
}

// ============================================================================
// Reference::new_ast() API Tests
// ============================================================================

#[test]
fn test_reference_new_ast_api() {
    // Test that Reference::new_ast() works identically to extract_references_from_ast()
    let text = "Check [md link](other.md) and [another](docs/readme.md#intro).";
    let file_name = "test";

    let api_refs: Vec<_> = Reference::new_ast(text, file_name).collect();
    let direct_refs: Vec<_> = extract_references_from_ast(text, file_name);

    assert_eq!(api_refs.len(), direct_refs.len());
    for (api_ref, direct_ref) in api_refs.iter().zip(direct_refs.iter()) {
        assert!(refs_equal(api_ref, direct_ref));
    }
}

#[test]
fn test_reference_new_ast_vs_regex() {
    // Test that Reference::new_ast() produces results equivalent to Reference::new()
    // for non-tag references
    let text = r#"# Document

Check [md link](other.md) and [another](doc.md#section).

See footnote[^1] and reference [ref].

[ref]: http://example.com

[^1]: Footnote text
"#;
    let file_name = "test";

    let mut regex_refs: Vec<_> = filter_tags(Reference::new(text, file_name).collect());
    let mut ast_refs: Vec<_> = Reference::new_ast(text, file_name).collect();

    sort_refs(&mut regex_refs);
    sort_refs(&mut ast_refs);

    assert!(
        compare_refs(&regex_refs, &ast_refs),
        "Reference::new_ast() should match Reference::new() for non-tag references"
    );
}
