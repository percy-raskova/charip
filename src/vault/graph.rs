//! Graph-based representation of the vault structure.
//!
//! This module provides a `petgraph`-based graph representation of the vault,
//! where each document is a node and relationships (references, toctree, include)
//! are edges.
//!
//! # Design Rationale
//!
//! The vault is inherently a graph structure:
//! - Documents reference each other via links, roles, and anchors
//! - Sphinx toctrees create parent-child relationships
//! - Include directives create transclusion relationships
//!
//! Using `petgraph::DiGraph` enables:
//! - Efficient traversal (DFS, BFS)
//! - Cycle detection (important for include loops)
//! - Path queries (shortest path between documents)
//! - Graph algorithms (connected components, topological sort)
//!
//! # Phase 1: Foundation Types
//!
//! This initial implementation establishes the core types without modifying
//! existing vault functionality. Future phases will:
//! - Phase 2: Add graph construction from existing vault
//! - Phase 3: Migrate queries to graph-based operations
//! - Phase 4: Add toctree/include edge extraction

use petgraph::prelude::*;
use std::iter;
use std::path::PathBuf;

use super::types::{
    MDFootnote, MDHeading, MDIndexedBlock, MDLinkReferenceDefinition, MDSubstitutionDef, MDTag,
};
use super::{metadata::MDMetadata, parsing::MDCodeBlock, MDFile, Reference, Referenceable};
use crate::myst_parser::{GlossaryTerm, MystSymbol, MystSymbolKind};

/// A parsed Markdown document stored as a graph node.
///
/// `DocumentNode` represents a single `.md` file in the vault. It contains
/// all extracted elements: headings, references, anchors, tags, and metadata.
/// These nodes are stored in the vault's [`VaultGraph`] with edges representing
/// inter-document relationships.
///
/// # Contents
///
/// A document node contains:
///
/// | Field | Description |
/// |-------|-------------|
/// | `references` | All outgoing links (also stored as graph edges) |
/// | `headings` | Section headings for navigation |
/// | `indexed_blocks` | Blocks marked with `^id` for direct linking |
/// | `tags` | Hashtag categorization |
/// | `myst_symbols` | MyST directives and anchors |
/// | `glossary_terms` | Terms from `{glossary}` directives |
///
/// # Graph Storage
///
/// In the vault graph:
/// - This struct is the **node weight** (`DiGraph<DocumentNode, EdgeKind>`)
/// - References between files become **edges** for efficient backlink queries
/// - Access via [`Vault::get_document()`](super::Vault::get_document)
///
/// # Example
///
/// ```rust,ignore
/// let node = vault.get_document(&path).unwrap();
/// for heading in &node.headings {
///     println!("## {}", heading.heading_text);
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentNode {
    /// Absolute path to the Markdown file
    pub path: PathBuf,
    /// All references in this file (links, tags, footnotes, etc.)
    ///
    /// Note: File-to-file references that resolve are ALSO stored as graph edges
    /// for efficient backlink queries. This field contains ALL references.
    pub references: Vec<Reference>,
    /// Parsed headings (# Title, ## Section, etc.)
    pub headings: Vec<MDHeading>,
    /// Indexed blocks (^block-id)
    pub indexed_blocks: Vec<MDIndexedBlock>,
    /// Tag references (#topic, #project/subtopic)
    pub tags: Vec<MDTag>,
    /// Footnote definitions ([^1]: footnote text)
    pub footnotes: Vec<MDFootnote>,
    /// Link reference definitions ([label]: url)
    pub link_reference_definitions: Vec<MDLinkReferenceDefinition>,
    /// Parsed frontmatter metadata
    pub metadata: Option<MDMetadata>,
    /// Code blocks (```lang ... ```)
    pub codeblocks: Vec<MDCodeBlock>,
    /// MyST symbols (directives, anchors)
    pub myst_symbols: Vec<MystSymbol>,
    /// Glossary terms from {glossary} directives
    pub glossary_terms: Vec<GlossaryTerm>,
    /// Substitution definitions from frontmatter
    pub substitution_defs: Vec<MDSubstitutionDef>,
}

impl Default for DocumentNode {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            references: vec![],
            headings: vec![],
            indexed_blocks: vec![],
            tags: vec![],
            footnotes: vec![],
            link_reference_definitions: vec![],
            metadata: None,
            codeblocks: vec![],
            myst_symbols: vec![],
            glossary_terms: vec![],
            substitution_defs: vec![],
        }
    }
}

impl DocumentNode {
    /// Returns the file name (stem) without extension.
    ///
    /// Mirrors `MDFile::file_name()` for API compatibility.
    pub fn file_name(&self) -> Option<&str> {
        self.path.file_stem()?.to_str()
    }

    /// Get all referenceables from this document node.
    ///
    /// Returns a vector of all things that can be referenced in this file:
    /// - The file itself
    /// - Headings
    /// - Indexed blocks
    /// - Tags
    /// - Footnotes
    /// - Link reference definitions
    /// - MyST anchors
    /// - Glossary terms
    /// - Math labels
    /// - Substitution definitions
    /// - Directive labels (directives with :name: or :label: options)
    pub fn get_referenceables(&self) -> Vec<Referenceable<'_>> {
        let DocumentNode {
            path: _,
            references: _, // References are accessed separately via edges or doc.references
            headings,
            indexed_blocks,
            tags,
            footnotes,
            link_reference_definitions,
            metadata: _,
            codeblocks: _,
            myst_symbols,
            glossary_terms,
            substitution_defs,
        } = self;

        iter::once(Referenceable::File(&self.path, self))
            .chain(
                headings
                    .iter()
                    .map(|heading| Referenceable::Heading(&self.path, heading)),
            )
            .chain(
                indexed_blocks
                    .iter()
                    .map(|block| Referenceable::IndexedBlock(&self.path, block)),
            )
            .chain(tags.iter().map(|tag| Referenceable::Tag(&self.path, tag)))
            .chain(
                footnotes
                    .iter()
                    .map(|footnote| Referenceable::Footnote(&self.path, footnote)),
            )
            .chain(
                link_reference_definitions
                    .iter()
                    .map(|link_ref| Referenceable::LinkRefDef(&self.path, link_ref)),
            )
            .chain(
                myst_symbols
                    .iter()
                    .filter(|s| s.kind == MystSymbolKind::Anchor)
                    .map(|anchor| Referenceable::MystAnchor(&self.path, anchor)),
            )
            .chain(
                glossary_terms
                    .iter()
                    .map(|term| Referenceable::GlossaryTerm(&self.path, term)),
            )
            .chain(
                myst_symbols
                    .iter()
                    .filter(|s| {
                        s.kind == MystSymbolKind::Directive && s.name == "math" && s.label.is_some()
                    })
                    .map(|math_sym| Referenceable::MathLabel(&self.path, math_sym)),
            )
            .chain(
                substitution_defs
                    .iter()
                    .map(|sub_def| Referenceable::SubstitutionDef(&self.path, sub_def)),
            )
            .chain(
                // Directive labels: directives with :name: or :label: option
                // (excluding math directives which are handled separately as MathLabel)
                myst_symbols
                    .iter()
                    .filter(|s| {
                        s.kind == MystSymbolKind::Directive
                            && s.label.is_some()
                            && s.name != "math" // math labels are handled separately
                    })
                    .map(|directive| Referenceable::DirectiveLabel(&self.path, directive)),
            )
            .collect()
    }
}

/// Convert an MDFile reference to a DocumentNode.
///
/// This conversion copies all document-level data including references.
///
/// # Note on References and Graph Edges
///
/// References are stored in DocumentNode for direct access (e.g., listing all
/// references in a file). Additionally, file-to-file references that resolve
/// are stored as graph edges for efficient backlink queries. This intentional
/// duplication enables both use cases:
/// - DocumentNode.references: all references for a file (for completions, diagnostics)
/// - Graph edges: resolved file-to-file references (for backlinks)
impl From<&MDFile> for DocumentNode {
    fn from(file: &MDFile) -> Self {
        DocumentNode {
            path: file.path.clone(),
            references: file.references.clone(),
            headings: file.headings.clone(),
            indexed_blocks: file.indexed_blocks.clone(),
            tags: file.tags.clone(),
            footnotes: file.footnotes.clone(),
            link_reference_definitions: file.link_reference_definitions.clone(),
            metadata: file.metadata.clone(),
            codeblocks: file.codeblocks.clone(),
            myst_symbols: file.myst_symbols.clone(),
            glossary_terms: file.glossary_terms.clone(),
            substitution_defs: file.substitution_defs.clone(),
        }
    }
}

/// Edge data representing relationships between documents.
///
/// The vault graph has three types of edges:
/// - **Reference**: A link from one document to another (markdown links, roles)
/// - **Toctree**: A structural parent-child relationship from Sphinx toctrees
/// - **Include**: A transclusion relationship from include directives
///
/// # Example
///
/// ```ignore
/// // A reference edge from one doc to another
/// let edge = EdgeKind::Reference {
///     reference: Reference::MDFileLink(...),
///     source_path: PathBuf::from("/vault/source.md"),
/// };
///
/// // A toctree edge (parent -> child in documentation structure)
/// let edge = EdgeKind::Toctree { caption: Some("Getting Started".to_string()) };
/// ```
#[allow(dead_code)] // Phase 1: Foundation types for future graph migration
#[derive(Debug, Clone)]
pub enum EdgeKind {
    /// Reference edge: links, roles, footnotes
    Reference {
        reference: Reference,
        source_path: PathBuf,
    },
    /// Structural edge: toctree parent-child
    Toctree { caption: Option<String> },
    /// Transclusion edge: include directive
    Include,
}

/// The vault graph type alias.
///
/// A directed graph where:
/// - Nodes are `DocumentNode` (parsed document content)
/// - Edges are `EdgeKind` (relationships between documents)
///
/// Directionality matters:
/// - Reference edges point from source to target
/// - Toctree edges point from parent to child
/// - Include edges point from includer to includee
#[allow(dead_code)] // Phase 1: Foundation types for future graph migration
pub type VaultGraph = DiGraph<DocumentNode, EdgeKind>;

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Chunk 1.1: Core Graph Types Tests
    // ========================================================================

    #[test]
    fn test_document_node_creation() {
        let node = DocumentNode {
            path: PathBuf::from("/vault/test.md"),
            references: vec![],
            headings: vec![],
            indexed_blocks: vec![],
            tags: vec![],
            footnotes: vec![],
            link_reference_definitions: vec![],
            metadata: None,
            codeblocks: vec![],
            myst_symbols: vec![],
            glossary_terms: vec![],
            substitution_defs: vec![],
        };
        assert_eq!(node.path, PathBuf::from("/vault/test.md"));
    }

    #[test]
    fn test_edge_kind_toctree() {
        let edge = EdgeKind::Toctree {
            caption: Some("Chapter 1".to_string()),
        };
        assert!(matches!(edge, EdgeKind::Toctree { caption: Some(_) }));
    }

    #[test]
    fn test_edge_kind_toctree_no_caption() {
        let edge = EdgeKind::Toctree { caption: None };
        assert!(matches!(edge, EdgeKind::Toctree { caption: None }));
    }

    #[test]
    fn test_edge_kind_include() {
        let edge = EdgeKind::Include;
        assert!(matches!(edge, EdgeKind::Include));
    }

    // ========================================================================
    // Chunk 1.2: Default Implementation Tests
    // ========================================================================

    #[test]
    fn test_document_node_default() {
        let node = DocumentNode::default();
        assert!(node.path.as_os_str().is_empty());
        assert!(node.headings.is_empty());
        assert!(node.indexed_blocks.is_empty());
        assert!(node.tags.is_empty());
        assert!(node.footnotes.is_empty());
        assert!(node.link_reference_definitions.is_empty());
        assert!(node.metadata.is_none());
        assert!(node.codeblocks.is_empty());
        assert!(node.myst_symbols.is_empty());
        assert!(node.glossary_terms.is_empty());
        assert!(node.substitution_defs.is_empty());
    }

    #[test]
    fn test_document_node_default_with_path_override() {
        let node = DocumentNode {
            path: PathBuf::from("/custom/path.md"),
            ..Default::default()
        };
        assert_eq!(node.path, PathBuf::from("/custom/path.md"));
        assert!(node.headings.is_empty());
    }

    // ========================================================================
    // Chunk 1.3: Basic Graph Operations Tests
    // ========================================================================

    #[test]
    fn test_create_empty_graph() {
        let graph: VaultGraph = DiGraph::new();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_add_document_node() {
        let mut graph: VaultGraph = DiGraph::new();

        let node_idx = graph.add_node(DocumentNode {
            path: PathBuf::from("/vault/test.md"),
            ..Default::default()
        });

        assert_eq!(graph.node_count(), 1);
        assert_eq!(graph[node_idx].path, PathBuf::from("/vault/test.md"));
    }

    #[test]
    fn test_add_document_and_edge() {
        let mut graph: VaultGraph = DiGraph::new();

        let source = graph.add_node(DocumentNode {
            path: PathBuf::from("/source.md"),
            ..Default::default()
        });
        let target = graph.add_node(DocumentNode {
            path: PathBuf::from("/target.md"),
            ..Default::default()
        });

        graph.add_edge(source, target, EdgeKind::Include);

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_graph_neighbors() {
        let mut graph: VaultGraph = DiGraph::new();

        let a = graph.add_node(DocumentNode {
            path: PathBuf::from("/a.md"),
            ..Default::default()
        });
        let b = graph.add_node(DocumentNode {
            path: PathBuf::from("/b.md"),
            ..Default::default()
        });
        let c = graph.add_node(DocumentNode {
            path: PathBuf::from("/c.md"),
            ..Default::default()
        });

        graph.add_edge(a, b, EdgeKind::Include);
        graph.add_edge(a, c, EdgeKind::Toctree { caption: None });

        let neighbors: Vec<_> = graph.neighbors(a).collect();
        assert_eq!(neighbors.len(), 2);
    }

    #[test]
    fn test_graph_incoming_edges() {
        let mut graph: VaultGraph = DiGraph::new();

        let a = graph.add_node(DocumentNode {
            path: PathBuf::from("/a.md"),
            ..Default::default()
        });
        let b = graph.add_node(DocumentNode {
            path: PathBuf::from("/b.md"),
            ..Default::default()
        });
        let c = graph.add_node(DocumentNode {
            path: PathBuf::from("/c.md"),
            ..Default::default()
        });

        // b and c both point to a
        graph.add_edge(b, a, EdgeKind::Include);
        graph.add_edge(c, a, EdgeKind::Include);

        // Check incoming edges to a (reverse direction)
        let incoming: Vec<_> = graph
            .neighbors_directed(a, petgraph::Direction::Incoming)
            .collect();
        assert_eq!(incoming.len(), 2);
    }

    #[test]
    fn test_graph_with_reference_edge() {
        use super::super::ReferenceData;

        let mut graph: VaultGraph = DiGraph::new();

        let source = graph.add_node(DocumentNode {
            path: PathBuf::from("/source.md"),
            ..Default::default()
        });
        let target = graph.add_node(DocumentNode {
            path: PathBuf::from("/target.md"),
            ..Default::default()
        });

        let reference = Reference::MDFileLink(ReferenceData {
            reference_text: "target".to_string(),
            display_text: None,
            range: Default::default(),
        });

        graph.add_edge(
            source,
            target,
            EdgeKind::Reference {
                reference,
                source_path: PathBuf::from("/source.md"),
            },
        );

        assert_eq!(graph.edge_count(), 1);

        // Verify we can inspect the edge
        let edge = graph.edge_weight(graph.find_edge(source, target).unwrap());
        assert!(matches!(edge, Some(EdgeKind::Reference { .. })));
    }

    #[test]
    fn test_graph_multiple_edge_types() {
        let mut graph: VaultGraph = DiGraph::new();

        let index = graph.add_node(DocumentNode {
            path: PathBuf::from("/index.md"),
            ..Default::default()
        });
        let chapter1 = graph.add_node(DocumentNode {
            path: PathBuf::from("/chapter1.md"),
            ..Default::default()
        });
        let shared = graph.add_node(DocumentNode {
            path: PathBuf::from("/_shared/common.md"),
            ..Default::default()
        });

        // Toctree edge: index -> chapter1
        graph.add_edge(
            index,
            chapter1,
            EdgeKind::Toctree {
                caption: Some("Introduction".to_string()),
            },
        );

        // Include edge: chapter1 -> shared
        graph.add_edge(chapter1, shared, EdgeKind::Include);

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }

    // ========================================================================
    // Chunk 2.2: From<&MDFile> for DocumentNode Tests
    // ========================================================================

    #[test]
    fn test_document_node_from_mdfile_empty() {
        // Test conversion of an empty MDFile
        let md_file = MDFile {
            path: PathBuf::from("/vault/empty.md"),
            references: vec![],
            headings: vec![],
            indexed_blocks: vec![],
            tags: vec![],
            footnotes: vec![],
            link_reference_definitions: vec![],
            metadata: None,
            codeblocks: vec![],
            myst_symbols: vec![],
            glossary_terms: vec![],
            substitution_defs: vec![],
        };

        let node = DocumentNode::from(&md_file);

        assert_eq!(node.path, PathBuf::from("/vault/empty.md"));
        assert!(node.headings.is_empty());
        assert!(node.indexed_blocks.is_empty());
        assert!(node.tags.is_empty());
        assert!(node.footnotes.is_empty());
        assert!(node.link_reference_definitions.is_empty());
        assert!(node.metadata.is_none());
        assert!(node.codeblocks.is_empty());
        assert!(node.myst_symbols.is_empty());
        assert!(node.glossary_terms.is_empty());
        assert!(node.substitution_defs.is_empty());
    }

    #[test]
    fn test_document_node_from_mdfile_with_headings() {
        use super::super::types::{HeadingLevel, MyRange};

        let md_file = MDFile {
            path: PathBuf::from("/vault/test.md"),
            headings: vec![
                MDHeading {
                    heading_text: "Introduction".to_string(),
                    range: MyRange::default(),
                    level: HeadingLevel(1),
                },
                MDHeading {
                    heading_text: "Details".to_string(),
                    range: MyRange::default(),
                    level: HeadingLevel(2),
                },
            ],
            ..Default::default()
        };

        let node = DocumentNode::from(&md_file);

        assert_eq!(node.path, PathBuf::from("/vault/test.md"));
        assert_eq!(node.headings.len(), 2);
        assert_eq!(node.headings[0].heading_text, "Introduction");
        assert_eq!(node.headings[0].level.0, 1);
        assert_eq!(node.headings[1].heading_text, "Details");
        assert_eq!(node.headings[1].level.0, 2);
    }

    #[test]
    fn test_document_node_from_mdfile_with_tags() {
        use super::super::types::MyRange;

        let md_file = MDFile {
            path: PathBuf::from("/vault/tagged.md"),
            tags: vec![
                MDTag {
                    tag_ref: "project".to_string(),
                    range: MyRange::default(),
                },
                MDTag {
                    tag_ref: "rust".to_string(),
                    range: MyRange::default(),
                },
            ],
            ..Default::default()
        };

        let node = DocumentNode::from(&md_file);

        assert_eq!(node.tags.len(), 2);
        assert_eq!(node.tags[0].tag_ref, "project");
        assert_eq!(node.tags[1].tag_ref, "rust");
    }

    #[test]
    fn test_document_node_from_mdfile_preserves_metadata() {
        use super::super::metadata::MDMetadata;

        let metadata = MDMetadata::new("---\nzkid: 20231201\ncategory: notes\n---\n");

        let md_file = MDFile {
            path: PathBuf::from("/vault/with_meta.md"),
            metadata: metadata.clone(),
            ..Default::default()
        };

        let node = DocumentNode::from(&md_file);

        assert!(node.metadata.is_some());
        // Verify metadata content is preserved
        assert_eq!(node.metadata, metadata);
    }

    #[test]
    fn test_document_node_from_mdfile_references_not_copied() {
        // DocumentNode intentionally does NOT copy references
        // References are stored as edges in the graph, not as node data
        // This test documents this design decision
        use super::super::ReferenceData;

        let md_file = MDFile {
            path: PathBuf::from("/vault/with_refs.md"),
            references: vec![Reference::MDFileLink(ReferenceData {
                reference_text: "other.md".to_string(),
                display_text: None,
                range: Default::default(),
            })],
            ..Default::default()
        };

        let node = DocumentNode::from(&md_file);

        // Node should have the path
        assert_eq!(node.path, PathBuf::from("/vault/with_refs.md"));
        // But DocumentNode does not have a references field - that's by design
        // References become edges in the graph, not node data
    }
}
