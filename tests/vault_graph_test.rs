//! Integration tests for Vault graph methods.
//!
//! These tests validate the graph-based vault methods that have ignored doc tests,
//! ensuring they work correctly from an external consumer perspective.
//!
//! Important: These methods operate on specific edge types:
//! - `detect_include_cycles()` → uses `{include}` directive edges
//! - `find_orphan_documents()` → uses `{toctree}` edges only
//! - `transitive_dependencies()` / `transitive_dependents()` → use all edge types

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use charip::config::Settings;
use charip::vault::Vault;

/// Helper: Create a temporary vault directory for testing.
fn create_test_vault_dir() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let vault_dir = temp_dir.path().join("vault");
    fs::create_dir(&vault_dir).expect("Failed to create vault subdirectory");
    (temp_dir, vault_dir)
}

// ============================================================================
// count_labels Tests
// ============================================================================

#[test]
fn test_count_labels_no_labels() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create a file with NO headings, no anchors, no blocks
    // Note: The parser requires at least some content
    fs::write(vault_dir.join("plain.md"), "Just plain text without any labels.").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let count = vault.count_labels();
    // No headings, no anchors, no indexed blocks = 0 labels
    assert_eq!(count, 0, "Document with no headings/anchors should have 0 labels");
}

#[test]
fn test_count_labels_with_headings() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create file with headings (headings count as labels)
    fs::write(
        vault_dir.join("headings.md"),
        "# Heading One\n\nContent\n\n## Heading Two\n\nMore content",
    )
    .unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let count = vault.count_labels();
    // 2 headings = 2 labels
    assert_eq!(count, 2, "Should count 2 headings as labels");
}

#[test]
fn test_count_labels_with_anchors() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create file with MyST anchors - same format as internal test_vault_extracts_myst_anchors
    // MyST anchors are (name)= at start of line
    fs::write(
        vault_dir.join("labeled.md"),
        "(my-anchor)=\n# Section\n\n(another-anchor)=\n## Another",
    )
    .unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    // Verify anchors are actually extracted
    let anchors = vault.select_myst_anchors(None);
    assert_eq!(anchors.len(), 2, "Should find 2 anchors");

    // count_labels counts headings + indexed blocks + anchors + directive labels
    // We have 2 headings + 2 anchors = 4 labels minimum
    let count = vault.count_labels();
    assert!(count >= 4, "Should count at least 4 labels (2 headings + 2 anchors), got {}", count);
}

// ============================================================================
// count_references Tests
// ============================================================================

#[test]
fn test_count_references_empty_vault() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("plain.md"), "No references here, just text.").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let count = vault.count_references();
    assert_eq!(count, 0, "Document with no links should have 0 references");
}

#[test]
fn test_count_references_with_links() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(
        vault_dir.join("source.md"),
        r#"Text here.

[Link to target](target.md)
[Another link](other.md)
"#,
    )
    .unwrap();
    fs::write(vault_dir.join("target.md"), "Target content").unwrap();
    fs::write(vault_dir.join("other.md"), "Other content").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let count = vault.count_references();
    assert!(count >= 2, "Should count at least 2 markdown links, got {}", count);
}

// ============================================================================
// transitive_dependencies Tests
// ============================================================================

#[test]
fn test_transitive_dependencies_no_links() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("isolated.md"), "No outgoing links here.").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let deps = vault.transitive_dependencies(&vault_dir.join("isolated.md"));
    assert!(deps.is_empty(), "File with no links should have no dependencies");
}

#[test]
fn test_transitive_dependencies_chain() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create a chain using markdown links: a -> b -> c
    fs::write(vault_dir.join("a.md"), "Go to [b](b.md)").unwrap();
    fs::write(vault_dir.join("b.md"), "Go to [c](c.md)").unwrap();
    fs::write(vault_dir.join("c.md"), "End of chain").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let deps = vault.transitive_dependencies(&vault_dir.join("a.md"));

    // a's dependencies should include b and c
    assert!(
        deps.contains(&vault_dir.join("b.md")),
        "a should transitively depend on b"
    );
    assert!(
        deps.contains(&vault_dir.join("c.md")),
        "a should transitively depend on c"
    );
}

// ============================================================================
// transitive_dependents Tests
// ============================================================================

#[test]
fn test_transitive_dependents_no_backlinks() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("lonely.md"), "Nobody links here.").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let dependents = vault.transitive_dependents(&vault_dir.join("lonely.md"));
    assert!(
        dependents.is_empty(),
        "File with no incoming links should have no dependents"
    );
}

#[test]
fn test_transitive_dependents_chain() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create a chain: a -> b -> c
    fs::write(vault_dir.join("a.md"), "Go to [b](b.md)").unwrap();
    fs::write(vault_dir.join("b.md"), "Go to [c](c.md)").unwrap();
    fs::write(vault_dir.join("c.md"), "End").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let dependents = vault.transitive_dependents(&vault_dir.join("c.md"));

    // c's dependents should include a and b
    assert!(
        dependents.contains(&vault_dir.join("b.md")),
        "c should have b as dependent"
    );
    assert!(
        dependents.contains(&vault_dir.join("a.md")),
        "c should have a as transitive dependent"
    );
}

// ============================================================================
// find_orphan_documents Tests
// Note: This uses TOCTREE edges only, not regular links or {doc} roles
// ============================================================================

#[test]
fn test_find_orphan_documents_all_in_toctree() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create an index with toctree that includes all pages
    fs::write(
        vault_dir.join("index.md"),
        "```{toctree}\npage1\npage2\n```",
    )
    .unwrap();
    fs::write(vault_dir.join("page1.md"), "Page 1 content").unwrap();
    fs::write(vault_dir.join("page2.md"), "Page 2 content").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let orphans = vault.find_orphan_documents(&vault_dir.join("index.md"));
    assert!(orphans.is_empty(), "All documents in toctree should not be orphans");
}

#[test]
fn test_find_orphan_documents_with_orphan() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create an index that only includes page1 in toctree
    fs::write(vault_dir.join("index.md"), "```{toctree}\npage1\n```").unwrap();
    fs::write(vault_dir.join("page1.md"), "Page 1 content").unwrap();
    fs::write(vault_dir.join("orphan.md"), "Not in any toctree!").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let orphans = vault.find_orphan_documents(&vault_dir.join("index.md"));
    assert!(
        orphans.contains(&vault_dir.join("orphan.md")),
        "orphan.md should be detected as orphan"
    );
}

// ============================================================================
// detect_include_cycles Tests
// Note: This uses INCLUDE directive edges only, not regular links
// ============================================================================

#[test]
fn test_detect_include_cycles_no_cycles() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Linear chain of includes, no cycles
    fs::write(vault_dir.join("a.md"), "```{include} b.md\n```").unwrap();
    fs::write(vault_dir.join("b.md"), "```{include} c.md\n```").unwrap();
    fs::write(vault_dir.join("c.md"), "End of chain").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let cycles = vault.detect_include_cycles();
    assert!(cycles.is_empty(), "Linear include chain should have no cycles");
}

#[test]
fn test_detect_include_cycles_with_cycle() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create a cycle: a -> b -> c -> a (using {include} directive)
    fs::write(vault_dir.join("a.md"), "```{include} b.md\n```").unwrap();
    fs::write(vault_dir.join("b.md"), "```{include} c.md\n```").unwrap();
    fs::write(vault_dir.join("c.md"), "```{include} a.md\n```").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let cycles = vault.detect_include_cycles();
    assert!(!cycles.is_empty(), "Should detect the a -> b -> c -> a cycle");
}

#[test]
fn test_detect_include_cycles_self_include() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Self-including document
    fs::write(vault_dir.join("self.md"), "```{include} self.md\n```").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let cycles = vault.detect_include_cycles();
    assert!(!cycles.is_empty(), "Self-include should be detected as a cycle");
}

#[test]
fn test_regular_links_not_include_cycles() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Regular markdown links forming a cycle should NOT be detected
    // because detect_include_cycles only looks at {include} edges
    fs::write(vault_dir.join("a.md"), "[link to b](b.md)").unwrap();
    fs::write(vault_dir.join("b.md"), "[link to a](a.md)").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    let cycles = vault.detect_include_cycles();
    assert!(
        cycles.is_empty(),
        "Regular link cycles should NOT be detected as include cycles"
    );
}
