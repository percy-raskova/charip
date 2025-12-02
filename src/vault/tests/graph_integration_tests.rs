//! Integration tests for Vault graph functionality.
//!
//! These tests verify that the Vault struct correctly integrates with the
//! petgraph-based graph storage alongside the existing HashMap storage.

use std::fs;

use crate::config::Settings;
use crate::test_utils::create_test_vault_dir;
use crate::vault::Vault;

// ============================================================================
// Chunk 2.1: Vault Graph Fields Tests
// ============================================================================

#[test]
fn test_vault_has_graph_field() {
    // Verify Vault struct has a graph field that can be accessed
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create a minimal vault
    fs::write(vault_dir.join("test.md"), "# Test").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // This test verifies the graph field exists and has the correct type
    // With Chunk 2.3, graph should have 1 node for test.md
    assert_eq!(vault.graph.node_count(), 1);
}

#[test]
fn test_vault_has_node_index_field() {
    // Verify Vault struct has a node_index field for path -> NodeIndex lookup
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create a minimal vault
    fs::write(vault_dir.join("test.md"), "# Test").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // This test verifies the node_index field exists
    // With Chunk 2.3, node_index should have 1 entry for test.md
    assert_eq!(vault.node_index.len(), 1);
}

#[test]
fn test_vault_graph_populated() {
    // Verify that the graph is populated (Phase 5: md_files removed)
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("file1.md"), "# File 1").unwrap();
    fs::write(vault_dir.join("file2.md"), "# File 2").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Graph storage should be populated
    assert_eq!(vault.node_index.len(), 2);
    assert_eq!(vault.graph.node_count(), 2);
}

// ============================================================================
// Chunk 2.3: construct_vault() Populates Graph Tests
// ============================================================================

#[test]
fn test_construct_vault_populates_graph_nodes() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("file1.md"), "# File 1").unwrap();
    fs::write(vault_dir.join("file2.md"), "# File 2").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Should have 2 nodes (one per file)
    assert_eq!(vault.graph.node_count(), 2);
}

#[test]
fn test_construct_vault_populates_node_index() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("alpha.md"), "# Alpha").unwrap();
    fs::write(vault_dir.join("beta.md"), "# Beta").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Should have node_index entries for both files
    assert_eq!(vault.node_index.len(), 2);
    assert!(vault.node_index.contains_key(&vault_dir.join("alpha.md")));
    assert!(vault.node_index.contains_key(&vault_dir.join("beta.md")));
}

#[test]
fn test_construct_vault_creates_reference_edges() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // source.md links to target.md
    fs::write(vault_dir.join("source.md"), "[link](target.md)").unwrap();
    fs::write(vault_dir.join("target.md"), "# Target").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Should have at least 1 edge (source -> target)
    assert!(
        vault.graph.edge_count() >= 1,
        "Expected at least 1 edge for the link"
    );
}

#[test]
fn test_construct_vault_graph_node_data_matches_mdfile() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    let content = r#"# My Document

## Section One

Some content here.

## Section Two

More content.
"#;
    fs::write(vault_dir.join("doc.md"), content).unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Get the node for doc.md
    let doc_path = vault_dir.join("doc.md");
    let node_idx = vault
        .node_index
        .get(&doc_path)
        .expect("doc.md should be in node_index");
    let node = &vault.graph[*node_idx];

    // Verify node data matches MDFile data
    assert_eq!(node.path, doc_path);
    assert_eq!(node.headings.len(), 3); // # My Document, ## Section One, ## Section Two
}

#[test]
fn test_construct_vault_multiple_edges_from_single_file() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // source.md has multiple links
    fs::write(
        vault_dir.join("source.md"),
        "[first](first.md)\n[second](second.md)",
    )
    .unwrap();
    fs::write(vault_dir.join("first.md"), "# First").unwrap();
    fs::write(vault_dir.join("second.md"), "# Second").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Should have at least 2 edges (source -> first, source -> second)
    assert!(
        vault.graph.edge_count() >= 2,
        "Expected at least 2 edges for multiple links"
    );
}

#[test]
fn test_construct_vault_edge_data_contains_reference() {
    use crate::vault::graph::EdgeKind;

    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("a.md"), "[link to b](b.md)").unwrap();
    fs::write(vault_dir.join("b.md"), "# B").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Find the edge from a to b
    let a_path = vault_dir.join("a.md");
    let b_path = vault_dir.join("b.md");
    let a_idx = vault.node_index.get(&a_path).expect("a.md should exist");
    let b_idx = vault.node_index.get(&b_path).expect("b.md should exist");

    let edge = vault.graph.find_edge(*a_idx, *b_idx);
    assert!(edge.is_some(), "Expected edge from a.md to b.md");

    // Verify the edge contains Reference data
    let edge_weight = vault.graph.edge_weight(edge.unwrap());
    assert!(matches!(edge_weight, Some(EdgeKind::Reference { .. })));
}

#[test]
fn test_construct_vault_unresolved_links_no_edge() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // source.md links to nonexistent.md
    fs::write(vault_dir.join("source.md"), "[broken](nonexistent.md)").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Should have 1 node (source.md), but no edges since target doesn't exist
    assert_eq!(vault.graph.node_count(), 1);
    assert_eq!(vault.graph.edge_count(), 0);
}

// ============================================================================
// Chunk 3.1: Graph-based Forward Reference Resolution Tests
// ============================================================================

#[test]
fn test_graph_based_file_link_resolution() {
    use crate::vault::{Reference, Referenceable};

    let (_temp_dir, vault_dir) = create_test_vault_dir();
    fs::write(vault_dir.join("source.md"), "[link](target.md)").unwrap();
    fs::write(vault_dir.join("target.md"), "# Target").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Get the reference from source.md
    let source_path = vault_dir.join("source.md");
    let source_refs = vault.select_references(Some(&source_path));
    let file_link = source_refs
        .iter()
        .find(|(_, r)| matches!(r, Reference::MDFileLink(_)))
        .map(|(_, r)| r)
        .expect("Should find file link reference");

    // Resolve via graph-optimized method
    let targets = vault.select_referenceables_for_reference(file_link, &source_path);

    assert_eq!(targets.len(), 1, "Should resolve to exactly one target");
    assert!(
        matches!(targets[0], Referenceable::File(..)),
        "Should resolve to a File referenceable"
    );

    // Verify it's the correct target file
    let target_path = targets[0].get_path();
    assert_eq!(
        target_path,
        vault_dir.join("target.md"),
        "Should resolve to target.md"
    );
}

#[test]
fn test_graph_based_heading_link_resolution() {
    use crate::vault::{Reference, Referenceable};

    let (_temp_dir, vault_dir) = create_test_vault_dir();
    // The heading reference must match the actual heading text (case-insensitive)
    // Note: The implementation does case-insensitive matching, not slug matching
    fs::write(
        vault_dir.join("source.md"),
        "[link](target.md#Introduction)",
    )
    .unwrap();
    fs::write(
        vault_dir.join("target.md"),
        "# Introduction\n\nContent here.",
    )
    .unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Get the heading link reference from source.md
    let source_path = vault_dir.join("source.md");
    let source_refs = vault.select_references(Some(&source_path));

    let heading_link = source_refs
        .iter()
        .find(|(_, r)| matches!(r, Reference::MDHeadingLink(..)))
        .map(|(_, r)| r)
        .expect("Should find heading link reference");

    // Resolve via graph-optimized method
    let targets = vault.select_referenceables_for_reference(heading_link, &source_path);

    assert_eq!(targets.len(), 1, "Should resolve to exactly one target");
    assert!(
        matches!(targets[0], Referenceable::Heading(..)),
        "Should resolve to a Heading referenceable"
    );
}

#[test]
fn test_graph_based_indexed_block_link_resolution() {
    use crate::vault::{Reference, Referenceable};

    let (_temp_dir, vault_dir) = create_test_vault_dir();
    fs::write(vault_dir.join("source.md"), "[link](target.md#^blockid)").unwrap();
    fs::write(vault_dir.join("target.md"), "This is a block ^blockid").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Get the indexed block link reference from source.md
    let source_path = vault_dir.join("source.md");
    let source_refs = vault.select_references(Some(&source_path));
    let block_link = source_refs
        .iter()
        .find(|(_, r)| matches!(r, Reference::MDIndexedBlockLink(..)))
        .map(|(_, r)| r)
        .expect("Should find indexed block link reference");

    // Resolve via graph-optimized method
    let targets = vault.select_referenceables_for_reference(block_link, &source_path);

    assert_eq!(targets.len(), 1, "Should resolve to exactly one target");
    assert!(
        matches!(targets[0], Referenceable::IndexedBlock(..)),
        "Should resolve to an IndexedBlock referenceable"
    );
}

#[test]
fn test_graph_fallback_for_tags() {
    use crate::vault::Referenceable;

    let (_temp_dir, vault_dir) = create_test_vault_dir();
    // Note: tags are parsed separately, not through the AST reference parser
    fs::write(vault_dir.join("tagged.md"), "Content #mytag here").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Get tag referenceables from the vault
    let referenceables = vault.select_referenceable_nodes(None);
    let tag_ref = referenceables
        .iter()
        .find(|r| matches!(r, Referenceable::Tag(..)))
        .expect("Should find tag referenceable");

    // The tag should have the correct refname
    let refname = tag_ref.get_refname(vault.root_dir());
    assert!(
        refname.is_some_and(|r| r.full_refname.contains("mytag")),
        "Tag refname should contain 'mytag'"
    );
}

// ============================================================================
// Chunk 3.2: Graph-based Backlinks Resolution Tests
// ============================================================================

#[test]
fn test_graph_based_backlinks_single_source() {
    use crate::vault::Referenceable;

    let (_temp_dir, vault_dir) = create_test_vault_dir();
    fs::write(vault_dir.join("target.md"), "# Target").unwrap();
    fs::write(vault_dir.join("source.md"), "[link](target.md)").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Get target as referenceable via graph
    let target_path = vault_dir.join("target.md");
    let target_node = vault.get_document(&target_path).unwrap();
    let referenceable = Referenceable::File(&target_path, target_node);

    // Find backlinks via graph-optimized method
    let backlinks = vault.select_references_for_referenceable(&referenceable);

    assert_eq!(backlinks.len(), 1, "Should find exactly one backlink");
    assert_eq!(
        backlinks[0].0,
        vault_dir.join("source.md"),
        "Backlink should come from source.md"
    );
}

#[test]
fn test_graph_based_backlinks_multiple_sources() {
    use crate::vault::Referenceable;

    let (_temp_dir, vault_dir) = create_test_vault_dir();
    fs::write(vault_dir.join("target.md"), "# Target").unwrap();
    fs::write(vault_dir.join("source1.md"), "[link](target.md)").unwrap();
    fs::write(vault_dir.join("source2.md"), "[another](target.md)").unwrap();
    fs::write(vault_dir.join("source3.md"), "No links here").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Get target as referenceable via graph
    let target_path = vault_dir.join("target.md");
    let target_node = vault.get_document(&target_path).unwrap();
    let referenceable = Referenceable::File(&target_path, target_node);

    // Find backlinks via graph-optimized method
    let backlinks = vault.select_references_for_referenceable(&referenceable);

    assert_eq!(backlinks.len(), 2, "Should find exactly two backlinks");

    // Verify the source paths (order may vary due to sorting)
    let source_paths: Vec<_> = backlinks.iter().map(|(p, _)| p).collect();
    assert!(
        source_paths.contains(&&vault_dir.join("source1.md").as_path()),
        "Should include source1.md"
    );
    assert!(
        source_paths.contains(&&vault_dir.join("source2.md").as_path()),
        "Should include source2.md"
    );
}

#[test]
fn test_graph_based_backlinks_to_heading() {
    use crate::vault::Referenceable;

    let (_temp_dir, vault_dir) = create_test_vault_dir();
    fs::write(
        vault_dir.join("target.md"),
        "# Introduction\n\nContent here.",
    )
    .unwrap();
    fs::write(
        vault_dir.join("source.md"),
        "[see intro](target.md#introduction)",
    )
    .unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Get the heading as referenceable via graph
    let target_path = vault_dir.join("target.md");
    let target_node = vault.get_document(&target_path).unwrap();
    let heading = &target_node.headings[0];
    let referenceable = Referenceable::Heading(&target_path, heading);

    // Find backlinks via graph-optimized method
    let backlinks = vault.select_references_for_referenceable(&referenceable);

    assert_eq!(
        backlinks.len(),
        1,
        "Should find exactly one backlink to heading"
    );
    assert_eq!(
        backlinks[0].0,
        vault_dir.join("source.md"),
        "Backlink should come from source.md"
    );
}

#[test]
fn test_graph_backlinks_no_results_for_unlinked_file() {
    use crate::vault::Referenceable;

    let (_temp_dir, vault_dir) = create_test_vault_dir();
    fs::write(
        vault_dir.join("orphan.md"),
        "# Orphan\n\nNo one links here.",
    )
    .unwrap();
    fs::write(vault_dir.join("other.md"), "# Other\n\nDifferent content.").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Get orphan as referenceable via graph
    let orphan_path = vault_dir.join("orphan.md");
    let orphan_node = vault.get_document(&orphan_path).unwrap();
    let referenceable = Referenceable::File(&orphan_path, orphan_node);

    // Find backlinks
    let backlinks = vault.select_references_for_referenceable(&referenceable);

    assert!(
        backlinks.is_empty(),
        "Should find no backlinks for orphan file"
    );
}

// ============================================================================
// Chunk 4.1: Toctree Edge Tracking Tests
// ============================================================================

#[test]
fn test_toctree_edges_created() {
    use crate::vault::graph::EdgeKind;

    let (_temp, vault_dir) = create_test_vault_dir();

    // Index with toctree
    fs::write(
        vault_dir.join("index.md"),
        r#"# Index

```{toctree}
:caption: Contents

chapter1
chapter2
```
"#,
    )
    .unwrap();
    fs::write(vault_dir.join("chapter1.md"), "# Chapter 1").unwrap();
    fs::write(vault_dir.join("chapter2.md"), "# Chapter 2").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Should have toctree edges from index to chapters
    let index_idx = vault
        .node_index
        .get(&vault_dir.join("index.md"))
        .expect("index.md should be indexed");
    let toctree_edges: Vec<_> = vault
        .graph
        .edges(*index_idx)
        .filter(|e| matches!(e.weight(), EdgeKind::Toctree { .. }))
        .collect();

    assert_eq!(toctree_edges.len(), 2);
}

#[test]
fn test_toctree_edge_with_caption() {
    use crate::vault::graph::EdgeKind;

    let (_temp, vault_dir) = create_test_vault_dir();

    fs::write(
        vault_dir.join("index.md"),
        r#"# Index

```{toctree}
:caption: Getting Started

intro
```
"#,
    )
    .unwrap();
    fs::write(vault_dir.join("intro.md"), "# Introduction").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    let index_idx = vault.node_index.get(&vault_dir.join("index.md")).unwrap();
    let toctree_edges: Vec<_> = vault
        .graph
        .edges(*index_idx)
        .filter_map(|e| {
            if let EdgeKind::Toctree { caption } = e.weight() {
                Some(caption.clone())
            } else {
                None
            }
        })
        .collect();

    assert_eq!(toctree_edges.len(), 1);
    assert_eq!(toctree_edges[0], Some("Getting Started".to_string()));
}

#[test]
fn test_toctree_edge_with_subdirectory_entry() {
    use crate::vault::graph::EdgeKind;

    let (_temp, vault_dir) = create_test_vault_dir();

    // Create subdirectory
    fs::create_dir(vault_dir.join("guides")).unwrap();

    fs::write(
        vault_dir.join("index.md"),
        r#"# Index

```{toctree}
guides/intro
```
"#,
    )
    .unwrap();
    fs::write(vault_dir.join("guides/intro.md"), "# Guide Intro").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    let index_idx = vault.node_index.get(&vault_dir.join("index.md")).unwrap();
    let toctree_edges: Vec<_> = vault
        .graph
        .edges(*index_idx)
        .filter(|e| matches!(e.weight(), EdgeKind::Toctree { .. }))
        .collect();

    assert_eq!(toctree_edges.len(), 1);
}

#[test]
fn test_toctree_unresolved_entries_no_edge() {
    use crate::vault::graph::EdgeKind;

    let (_temp, vault_dir) = create_test_vault_dir();

    fs::write(
        vault_dir.join("index.md"),
        r#"# Index

```{toctree}
existing
nonexistent
```
"#,
    )
    .unwrap();
    fs::write(vault_dir.join("existing.md"), "# Existing").unwrap();
    // nonexistent.md intentionally not created

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    let index_idx = vault.node_index.get(&vault_dir.join("index.md")).unwrap();
    let toctree_edges: Vec<_> = vault
        .graph
        .edges(*index_idx)
        .filter(|e| matches!(e.weight(), EdgeKind::Toctree { .. }))
        .collect();

    // Only 1 edge for the existing file
    assert_eq!(toctree_edges.len(), 1);
}

#[test]
fn test_multiple_toctrees_in_one_file() {
    use crate::vault::graph::EdgeKind;

    let (_temp, vault_dir) = create_test_vault_dir();

    fs::write(
        vault_dir.join("index.md"),
        r#"# Index

```{toctree}
:caption: Part 1

chapter1
```

```{toctree}
:caption: Part 2

chapter2
```
"#,
    )
    .unwrap();
    fs::write(vault_dir.join("chapter1.md"), "# Chapter 1").unwrap();
    fs::write(vault_dir.join("chapter2.md"), "# Chapter 2").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    let index_idx = vault.node_index.get(&vault_dir.join("index.md")).unwrap();
    let toctree_edges: Vec<_> = vault
        .graph
        .edges(*index_idx)
        .filter(|e| matches!(e.weight(), EdgeKind::Toctree { .. }))
        .collect();

    assert_eq!(toctree_edges.len(), 2);
}

// ============================================================================
// Chunk 4.2: Include Cycle Detection Tests
// ============================================================================

#[test]
fn test_detect_include_cycle() {
    let (_temp, vault_dir) = create_test_vault_dir();

    // Create a cycle: a includes b, b includes c, c includes a
    fs::write(vault_dir.join("a.md"), "```{include} b.md\n```").unwrap();
    fs::write(vault_dir.join("b.md"), "```{include} c.md\n```").unwrap();
    fs::write(vault_dir.join("c.md"), "```{include} a.md\n```").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let cycles = vault.detect_include_cycles();

    assert!(!cycles.is_empty(), "Should detect the include cycle");
    assert_eq!(cycles[0].len(), 3, "Cycle should contain 3 files");
}

#[test]
fn test_no_cycle_without_include() {
    let (_temp, vault_dir) = create_test_vault_dir();

    // Regular links, not includes
    fs::write(vault_dir.join("a.md"), "[link](b.md)").unwrap();
    fs::write(vault_dir.join("b.md"), "[link](a.md)").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let cycles = vault.detect_include_cycles();

    assert!(
        cycles.is_empty(),
        "Reference cycles are OK, only include cycles matter"
    );
}

#[test]
fn test_no_cycle_in_linear_include_chain() {
    let (_temp, vault_dir) = create_test_vault_dir();

    // Linear chain: a includes b, b includes c (no cycle)
    fs::write(vault_dir.join("a.md"), "```{include} b.md\n```").unwrap();
    fs::write(vault_dir.join("b.md"), "```{include} c.md\n```").unwrap();
    fs::write(vault_dir.join("c.md"), "# End of chain").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let cycles = vault.detect_include_cycles();

    assert!(
        cycles.is_empty(),
        "Linear chain should not be detected as cycle"
    );
}

#[test]
fn test_detect_self_include_cycle() {
    let (_temp, vault_dir) = create_test_vault_dir();

    // Self-include: a includes a
    fs::write(vault_dir.join("a.md"), "```{include} a.md\n```").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let cycles = vault.detect_include_cycles();

    assert!(!cycles.is_empty(), "Should detect self-include cycle");
}

// ============================================================================
// Chunk 4.3: Orphan Node Detection Tests
// ============================================================================

#[test]
fn test_find_orphan_documents() {
    let (_temp, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("index.md"), "```{toctree}\nchapter1\n```").unwrap();
    fs::write(vault_dir.join("chapter1.md"), "# Chapter 1").unwrap();
    fs::write(vault_dir.join("orphan.md"), "# Orphan - not in toctree").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let orphans = vault.find_orphan_documents(&vault_dir.join("index.md"));

    assert!(orphans.contains(&vault_dir.join("orphan.md")));
    assert!(!orphans.contains(&vault_dir.join("chapter1.md")));
    assert!(!orphans.contains(&vault_dir.join("index.md")));
}

#[test]
fn test_find_orphan_documents_nested_toctree() {
    let (_temp, vault_dir) = create_test_vault_dir();

    // index -> chapter1 -> section1
    fs::write(vault_dir.join("index.md"), "```{toctree}\nchapter1\n```").unwrap();
    fs::write(
        vault_dir.join("chapter1.md"),
        "# Chapter 1\n\n```{toctree}\nsection1\n```",
    )
    .unwrap();
    fs::write(vault_dir.join("section1.md"), "# Section 1").unwrap();
    fs::write(vault_dir.join("orphan.md"), "# Orphan").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let orphans = vault.find_orphan_documents(&vault_dir.join("index.md"));

    assert!(orphans.contains(&vault_dir.join("orphan.md")));
    assert!(!orphans.contains(&vault_dir.join("section1.md")));
    assert!(!orphans.contains(&vault_dir.join("chapter1.md")));
    assert!(!orphans.contains(&vault_dir.join("index.md")));
}

#[test]
fn test_find_orphan_no_root_returns_empty() {
    let (_temp, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("doc1.md"), "# Doc 1").unwrap();
    fs::write(vault_dir.join("doc2.md"), "# Doc 2").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    // Root doesn't exist
    let orphans = vault.find_orphan_documents(&vault_dir.join("nonexistent.md"));

    assert!(orphans.is_empty());
}

#[test]
fn test_find_orphan_all_connected_returns_empty() {
    let (_temp, vault_dir) = create_test_vault_dir();

    fs::write(
        vault_dir.join("index.md"),
        "```{toctree}\nchapter1\nchapter2\n```",
    )
    .unwrap();
    fs::write(vault_dir.join("chapter1.md"), "# Chapter 1").unwrap();
    fs::write(vault_dir.join("chapter2.md"), "# Chapter 2").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let orphans = vault.find_orphan_documents(&vault_dir.join("index.md"));

    // All files are connected, no orphans
    assert!(orphans.is_empty());
}

// ============================================================================
// Chunk 4.4: Transitive Dependency Queries Tests
// ============================================================================

#[test]
fn test_transitive_dependencies() {
    let (_temp, vault_dir) = create_test_vault_dir();

    // a -> b -> c -> d chain
    fs::write(vault_dir.join("a.md"), "[link](b.md)").unwrap();
    fs::write(vault_dir.join("b.md"), "[link](c.md)").unwrap();
    fs::write(vault_dir.join("c.md"), "[link](d.md)").unwrap();
    fs::write(vault_dir.join("d.md"), "# End").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let deps = vault.transitive_dependencies(&vault_dir.join("a.md"));

    assert!(deps.contains(&vault_dir.join("b.md")));
    assert!(deps.contains(&vault_dir.join("c.md")));
    assert!(deps.contains(&vault_dir.join("d.md")));
    assert!(!deps.contains(&vault_dir.join("a.md")));
}

#[test]
fn test_transitive_dependencies_branching() {
    let (_temp, vault_dir) = create_test_vault_dir();

    // a -> b, a -> c, b -> d, c -> d (diamond pattern)
    fs::write(vault_dir.join("a.md"), "[link](b.md)\n[link](c.md)").unwrap();
    fs::write(vault_dir.join("b.md"), "[link](d.md)").unwrap();
    fs::write(vault_dir.join("c.md"), "[link](d.md)").unwrap();
    fs::write(vault_dir.join("d.md"), "# End").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let deps = vault.transitive_dependencies(&vault_dir.join("a.md"));

    assert!(deps.contains(&vault_dir.join("b.md")));
    assert!(deps.contains(&vault_dir.join("c.md")));
    assert!(deps.contains(&vault_dir.join("d.md")));
}

#[test]
fn test_transitive_dependencies_isolated_file() {
    let (_temp, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("isolated.md"), "# No links").unwrap();
    fs::write(vault_dir.join("other.md"), "# Other").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let deps = vault.transitive_dependencies(&vault_dir.join("isolated.md"));

    assert!(deps.is_empty());
}

#[test]
fn test_transitive_dependents() {
    let (_temp, vault_dir) = create_test_vault_dir();

    // a -> d, b -> d, c -> d (d has 3 dependents)
    fs::write(vault_dir.join("a.md"), "[link](d.md)").unwrap();
    fs::write(vault_dir.join("b.md"), "[link](d.md)").unwrap();
    fs::write(vault_dir.join("c.md"), "[link](d.md)").unwrap();
    fs::write(vault_dir.join("d.md"), "# Target").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let dependents = vault.transitive_dependents(&vault_dir.join("d.md"));

    assert!(dependents.contains(&vault_dir.join("a.md")));
    assert!(dependents.contains(&vault_dir.join("b.md")));
    assert!(dependents.contains(&vault_dir.join("c.md")));
    assert!(!dependents.contains(&vault_dir.join("d.md")));
}

#[test]
fn test_transitive_dependents_chain() {
    let (_temp, vault_dir) = create_test_vault_dir();

    // a -> b -> c -> d (d is transitively depended on by a, b, c)
    fs::write(vault_dir.join("a.md"), "[link](b.md)").unwrap();
    fs::write(vault_dir.join("b.md"), "[link](c.md)").unwrap();
    fs::write(vault_dir.join("c.md"), "[link](d.md)").unwrap();
    fs::write(vault_dir.join("d.md"), "# End").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();
    let dependents = vault.transitive_dependents(&vault_dir.join("d.md"));

    assert!(dependents.contains(&vault_dir.join("a.md")));
    assert!(dependents.contains(&vault_dir.join("b.md")));
    assert!(dependents.contains(&vault_dir.join("c.md")));
}

#[test]
fn test_transitive_nonexistent_file() {
    let (_temp, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("a.md"), "# A").unwrap();

    let vault = Vault::construct_vault(&Settings::default(), &vault_dir).unwrap();

    let deps = vault.transitive_dependencies(&vault_dir.join("nonexistent.md"));
    assert!(deps.is_empty());

    let dependents = vault.transitive_dependents(&vault_dir.join("nonexistent.md"));
    assert!(dependents.is_empty());
}
