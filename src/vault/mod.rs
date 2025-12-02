mod ast_refs;
pub mod graph;
mod helpers;
mod metadata;
mod parsing;
mod types;

#[cfg(test)]
mod tests;

pub use helpers::{get_relative_ref_path, Refname};
pub use types::{
    HeadingLevel, MDFootnote, MDHeading, MDIndexedBlock, MDLinkReferenceDefinition,
    MDSubstitutionDef, MDTag, MyRange, Rangeable,
};

use std::{
    char,
    collections::{HashMap, HashSet},
    hash::Hash,
    iter,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    time::SystemTime,
};

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;

use self::graph::{DocumentNode, EdgeKind, VaultGraph};

use itertools::Itertools;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use ropey::Rope;
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{
    DocumentChangeOperation, Location, OneOf, OptionalVersionedTextDocumentIdentifier, Position,
    RenameFile, ResourceOp, SymbolInformation, SymbolKind, TextDocumentEdit, TextEdit, Url,
};
use walkdir::WalkDir;

impl Vault {
    pub fn construct_vault(context: &Settings, root_dir: &Path) -> Result<Vault, std::io::Error> {
        let md_file_paths = WalkDir::new(root_dir)
            .into_iter()
            .filter_entry(|e| {
                // Skip hidden directories (starting with '.')
                !e.file_name().to_str().is_some_and(|s| s.starts_with('.'))
            })
            .flatten()
            .filter(|f| f.path().extension().and_then(|e| e.to_str()) == Some("md"))
            .collect_vec();

        let md_files: HashMap<PathBuf, MDFile> = md_file_paths
            .par_iter()
            .flat_map(|p| {
                let text = std::fs::read_to_string(p.path())?;
                let md_file = MDFile::new(context, &text, PathBuf::from(p.path()));

                Ok::<(PathBuf, MDFile), std::io::Error>((p.path().into(), md_file))
            })
            .collect();

        let ropes: HashMap<PathBuf, Rope> = md_file_paths
            .iter()
            .flat_map(|p| {
                let text = std::fs::read_to_string(p.path())?;
                let rope = Rope::from_str(&text);

                Ok::<(PathBuf, Rope), std::io::Error>((p.path().into(), rope))
            })
            .collect();

        // Phase 2.3: Build graph from md_files
        let mut graph = VaultGraph::new();
        let mut node_index = HashMap::new();

        // First pass: create all nodes
        for (path, md_file) in &md_files {
            let node = DocumentNode::from(md_file);
            let idx = graph.add_node(node);
            node_index.insert(path.clone(), idx);
        }

        // Second pass: create edges from references
        for (source_path, md_file) in &md_files {
            if let Some(&source_idx) = node_index.get(source_path) {
                for reference in &md_file.references {
                    // Try to resolve the reference to a target file
                    if let Some(target_path) =
                        resolve_reference_target(reference, source_path, root_dir, &md_files)
                    {
                        if let Some(&target_idx) = node_index.get(&target_path) {
                            graph.add_edge(
                                source_idx,
                                target_idx,
                                EdgeKind::Reference {
                                    reference: reference.clone(),
                                    source_path: source_path.clone(),
                                },
                            );
                        }
                    }
                }
            }
        }

        // Third pass: create edges from toctree/include directives
        for (source_path, rope) in &ropes {
            if let Some(&source_idx) = node_index.get(source_path) {
                let text = rope.to_string();
                let toctrees = myst_parser::parse_toctrees(&text);

                for toctree in toctrees {
                    for entry in &toctree.entries {
                        // Resolve toctree entry to file path
                        if let Some(target_path) =
                            resolve_toctree_entry(entry, source_path, root_dir, &md_files)
                        {
                            if let Some(&target_idx) = node_index.get(&target_path) {
                                graph.add_edge(
                                    source_idx,
                                    target_idx,
                                    EdgeKind::Toctree {
                                        caption: toctree.caption.clone(),
                                    },
                                );
                            }
                        }
                    }
                }

                // Also create edges for include directives
                let includes = myst_parser::parse_includes(&text);
                for include_path in includes {
                    if let Some(target_path) =
                        resolve_toctree_entry(&include_path, source_path, root_dir, &md_files)
                    {
                        if let Some(&target_idx) = node_index.get(&target_path) {
                            graph.add_edge(source_idx, target_idx, EdgeKind::Include);
                        }
                    }
                }
            }
        }

        Ok(Vault {
            ropes: ropes.into(),
            root_dir: root_dir.into(),
            graph,
            node_index,
        })
    }

    pub fn update_vault(context: &Settings, old: &mut Vault, new_file: (&PathBuf, &str)) {
        let new_md_file = MDFile::new(context, new_file.1, new_file.0.clone());
        let new_doc_node = DocumentNode::from(&new_md_file);
        let source_path = new_file.0.clone();

        // Update or insert into graph
        if let Some(&idx) = old.node_index.get(&source_path) {
            // Remove all outgoing edges from this node (they'll be recreated)
            let outgoing: Vec<_> = old.graph.edges(idx).map(|e| e.id()).collect();
            for edge_id in outgoing {
                old.graph.remove_edge(edge_id);
            }
            // Update the node
            old.graph[idx] = new_doc_node;
        } else {
            // Add new node
            let idx = old.graph.add_node(new_doc_node);
            old.node_index.insert(source_path.clone(), idx);
        }

        // Re-add edges for resolved references
        if let Some(&source_idx) = old.node_index.get(&source_path) {
            for reference in &new_md_file.references {
                if let Some(target_path) =
                    resolve_reference_target(reference, &source_path, &old.root_dir, old)
                {
                    if let Some(&target_idx) = old.node_index.get(&target_path) {
                        old.graph.add_edge(
                            source_idx,
                            target_idx,
                            graph::EdgeKind::Reference {
                                reference: reference.clone(),
                                source_path: source_path.clone(),
                            },
                        );
                    }
                }
            }
        }

        // Update ropes
        let new_rope = Rope::from_str(new_file.1);
        let rope_entry = old.ropes.get_mut(new_file.0);

        match rope_entry {
            Some(rope) => {
                *rope = new_rope;
            }
            None => {
                old.ropes.insert(new_file.0.into(), new_rope);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MyHashMap<B: Hash>(HashMap<PathBuf, B>);

impl<B: Hash> Hash for MyHashMap<B> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // https://stackoverflow.com/questions/73195185/how-can-i-derive-hash-for-a-struct-containing-a-hashmap

        let mut pairs: Vec<_> = self.0.iter().collect();
        pairs.sort_by_key(|i| i.0);

        Hash::hash(&pairs, state);
    }
}

impl<B: Hash> Deref for MyHashMap<B> {
    type Target = HashMap<PathBuf, B>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// implement DerefMut
impl<B: Hash> DerefMut for MyHashMap<B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<B: Hash> From<HashMap<PathBuf, B>> for MyHashMap<B> {
    fn from(value: HashMap<PathBuf, B>) -> Self {
        MyHashMap(value)
    }
}

impl Hash for Vault {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the node_index keys (file paths) for Vault identity
        let mut paths: Vec<_> = self.node_index.keys().collect();
        paths.sort();
        paths.hash(state)
    }
}

fn find_range(referenceable: &Referenceable) -> Option<tower_lsp::lsp_types::Range> {
    match referenceable {
        Referenceable::File(..) => Some(tower_lsp::lsp_types::Range {
            start: tower_lsp::lsp_types::Position {
                line: 0,
                character: 0,
            },
            end: tower_lsp::lsp_types::Position {
                line: 0,
                character: 1,
            },
        }),
        _ => referenceable.get_range().map(|a_my_range| *a_my_range),
    }
}

#[derive(Debug, Clone)]
/// The in-memory representation of the Markdown vault files. This data is exposed through an interface of methods to select the vault's data.
/// These methods do not do any interpretation or analysis of the data. That is up to the consumer of this struct. The methods are analogous to selecting on a database.
///
/// # Storage Architecture
///
/// The vault uses a petgraph-based storage model:
/// - `graph`: Directed graph where nodes are `DocumentNode` and edges are `EdgeKind`
/// - `node_index`: HashMap for O(1) path-to-NodeIndex lookup
/// - `ropes`: Text content for efficient position mapping
///
/// Document data is accessed via the graph. References are stored in both
/// `DocumentNode.references` (for complete access) and as graph edges
/// (for efficient backlink queries).
pub struct Vault {
    /// Raw text content for each file
    pub ropes: MyHashMap<Rope>,
    /// Root directory of the vault
    root_dir: PathBuf,
    /// Graph-based storage for document relationships
    pub graph: VaultGraph,
    /// Lookup table: PathBuf -> NodeIndex for O(1) graph access
    pub node_index: HashMap<PathBuf, NodeIndex>,
}

/// Graph-based accessor methods (Phase 5: replaces md_files access)
impl Vault {
    /// Get document node by path (replaces md_files.get())
    pub fn get_document(&self, path: &Path) -> Option<&DocumentNode> {
        self.node_index.get(path).map(|&idx| &self.graph[idx])
    }

    /// Get mutable document node by path
    #[allow(dead_code)] // Public API for consumers
    pub fn get_document_mut(&mut self, path: &Path) -> Option<&mut DocumentNode> {
        self.node_index
            .get(path)
            .copied()
            .map(|idx| &mut self.graph[idx])
    }

    /// Iterate over all documents as (path, node) pairs
    pub fn documents(&self) -> impl Iterator<Item = (&PathBuf, &DocumentNode)> {
        self.node_index
            .iter()
            .map(|(path, &idx)| (path, &self.graph[idx]))
    }

    /// Get all document paths
    #[allow(dead_code)] // Public API for consumers
    pub fn document_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.node_index.keys()
    }

    /// Check if a path exists in the vault
    #[allow(dead_code)] // Public API for consumers
    pub fn contains_document(&self, path: &Path) -> bool {
        self.node_index.contains_key(path)
    }

    /// Get the number of documents in the vault
    #[allow(dead_code)] // Public API for consumers
    pub fn document_count(&self) -> usize {
        self.node_index.len()
    }
}

/// Methods using vaults data
impl Vault {
    /// Generic helper for selecting Vec fields from DocumentNode with optional path filtering.
    ///
    /// If `path` is Some, returns items from that file only.
    /// If `path` is None, returns items from all files in the vault.
    fn select_field<'a, T>(
        &'a self,
        path: Option<&'a Path>,
        extractor: impl Fn(&'a DocumentNode) -> &'a Vec<T>,
    ) -> Vec<(&'a Path, &'a T)> {
        match path {
            Some(path) => self
                .get_document(path)
                .map(&extractor)
                .map(|vec| vec.iter().map(|item| (path, item)).collect())
                .unwrap_or_default(),
            None => self
                .documents()
                .flat_map(|(path, doc)| extractor(doc).iter().map(|item| (path.as_path(), item)))
                .collect(),
        }
    }

    /// Select all references ([[link]] or #tag) in a file if path is Some, else all in vault.
    ///
    /// References are stored in DocumentNode.references for complete access.
    /// Note: Resolved file-to-file references are also stored as graph edges
    /// for efficient backlink queries, but this method returns the full list.
    pub fn select_references<'a>(
        &'a self,
        path: Option<&'a Path>,
    ) -> Vec<(&'a Path, &'a Reference)> {
        self.select_field(path, |doc| &doc.references)
    }

    /// Select all MyST symbols in a file if path is Some, else all in vault.
    ///
    /// This is a foundational method used by the convenience methods
    /// `select_myst_directives()` and `select_myst_anchors()`.
    #[allow(dead_code)] // Public API for LSP consumers and tests
    pub fn select_myst_symbols<'a>(
        &'a self,
        path: Option<&'a Path>,
    ) -> Vec<(&'a Path, &'a MystSymbol)> {
        self.select_field(path, |md| &md.myst_symbols)
    }

    /// Select MyST directives (` ```{note} `, ` ```{warning} `, etc.) in a file or vault.
    ///
    /// Convenience method that filters `select_myst_symbols()` by `MystSymbolKind::Directive`.
    #[allow(dead_code)] // Public API for LSP consumers and tests
    pub fn select_myst_directives<'a>(
        &'a self,
        path: Option<&'a Path>,
    ) -> Vec<(&'a Path, &'a MystSymbol)> {
        self.select_myst_symbols(path)
            .into_iter()
            .filter(|(_, s)| s.kind == MystSymbolKind::Directive)
            .collect()
    }

    /// Select MyST anchors (`(target-name)=`) in a file or vault.
    ///
    /// Convenience method that filters `select_myst_symbols()` by `MystSymbolKind::Anchor`.
    #[allow(dead_code)] // Public API for LSP consumers and tests
    pub fn select_myst_anchors<'a>(
        &'a self,
        path: Option<&'a Path>,
    ) -> Vec<(&'a Path, &'a MystSymbol)> {
        self.select_myst_symbols(path)
            .into_iter()
            .filter(|(_, s)| s.kind == MystSymbolKind::Anchor)
            .collect()
    }

    pub fn select_referenceable_at_position<'a>(
        &'a self,
        path: &'a Path,
        position: Position,
    ) -> Option<Referenceable<'a>> {
        // If no other referenceables are under the cursor, the file should be returned.

        let referenceable_nodes = self.select_referenceable_nodes(Some(path));

        let referenceable = referenceable_nodes
            .into_iter()
            .flat_map(|referenceable| Some((referenceable.clone(), referenceable.get_range()?)))
            .find(|(_, range)| {
                range.start.line <= position.line
                    && range.end.line >= position.line
                    && range.start.character <= position.character
                    && range.end.character >= position.character
            })
            .map(|tupl| tupl.0);

        match referenceable {
            None => self
                .node_index
                .get_key_value(path)
                .map(|(pathbuf, &idx)| Referenceable::File(pathbuf, &self.graph[idx])),
            _ => referenceable,
        }
    }

    pub fn select_reference_at_position<'a>(
        &'a self,
        path: &'a Path,
        position: Position,
    ) -> Option<&'a Reference> {
        let links = self.select_references(Some(path));

        let (_path, reference) = links.into_iter().find(|&l| {
            l.1.data().range.start.line <= position.line
            && l.1.data().range.end.line >= position.line
            && l.1.data().range.start.character <= position.character // this is a bug
            && l.1.data().range.end.character >= position.character
        })?;

        Some(reference)
    }

    /// Select all linkable positions in the vault
    pub fn select_referenceable_nodes<'a>(
        &'a self,
        path: Option<&'a Path>,
    ) -> Vec<Referenceable<'a>> {
        match path {
            Some(path) => {
                let resolved_referenceables =
                    iter::once(self.get_document(path).map(|doc| doc.get_referenceables()))
                        .flatten()
                        .flatten()
                        .collect_vec();

                resolved_referenceables

                // TODO: Add unresolved referenceables
            }
            None => {
                let resolved_referenceables = self
                    .documents()
                    .par_bridge()
                    .into_par_iter()
                    .flat_map(|(_, doc)| doc.get_referenceables())
                    .collect::<Vec<_>>();

                let resolved_referenceables_refnames: HashSet<String> = resolved_referenceables
                    .par_iter()
                    .flat_map(|resolved| {
                        resolved.get_refname(self.root_dir()).and_then(|refname| {
                            vec![
                                refname.to_string(),
                                format!(
                                    "{}{}",
                                    refname.link_file_key()?,
                                    refname
                                        .infile_ref
                                        .map(|refe| format!("#{}", refe))
                                        .unwrap_or("".to_string())
                                ),
                            ]
                            .into()
                        })
                    })
                    .flatten()
                    .collect();

                let references = self.select_references(None);
                let unresolved: Vec<_> = references
                    .iter()
                    .unique_by(|(_, reference)| &reference.data().reference_text)
                    .par_bridge()
                    .into_par_iter()
                    .filter(|(_, reference)| {
                        !resolved_referenceables_refnames.contains(&reference.data().reference_text)
                    })
                    .flat_map(|(_, reference)| match reference {
                        Reference::MDFileLink(data) => {
                            let mut path = self.root_dir().clone();
                            path.push(&reference.data().reference_text);

                            Some(Referenceable::UnresolvedFile(path, &data.reference_text))
                        }
                        Reference::MDHeadingLink(_data, end_path, heading) => {
                            let mut path = self.root_dir().clone();
                            path.push(end_path);

                            Some(Referenceable::UnresolvedHeading(path, end_path, heading))
                        }
                        Reference::MDIndexedBlockLink(_data, end_path, index) => {
                            let mut path = self.root_dir().clone();
                            path.push(end_path);

                            Some(Referenceable::UnresolvedIndexedBlock(path, end_path, index))
                        }
                        Reference::Tag(..)
                        | Reference::Footnote(..)
                        | Reference::LinkRef(..)
                        | Reference::MystRole(..)
                        | Reference::ImageLink(..)
                        | Reference::Substitution(..) => None, // Substitutions validated separately
                    })
                    .collect();

                resolved_referenceables
                    .into_iter()
                    .chain(unresolved)
                    .collect()
            }
        }
    }

    pub fn select_line(&self, path: &Path, line: isize) -> Option<Vec<char>> {
        let rope = self.ropes.get(path)?;

        let usize: usize = line.try_into().ok()?;

        rope.get_line(usize)
            .map(|slice| slice.chars().collect_vec())
    }

    pub fn select_headings(&self, path: &Path) -> Option<&Vec<MDHeading>> {
        let doc_node = self.get_document(path)?;
        Some(&doc_node.headings)
    }

    pub fn root_dir(&self) -> &PathBuf {
        &self.root_dir
    }

    /// Finds all references pointing to a given referenceable (backlinks).
    ///
    /// Uses graph-based resolution for file-based referenceables (O(K) where
    /// K = incoming edges to target file), with fallback to linear scan
    /// for reference types not well-suited for graph traversal (tags).
    ///
    /// Returns an empty `Vec` if no references are found.
    pub fn select_references_for_referenceable(
        &self,
        referenceable: &Referenceable,
    ) -> Vec<(&Path, &Reference)> {
        // Try graph-based backlink resolution first
        if let Some(results) = self.backlinks_via_graph(referenceable) {
            // Graph found results or confirmed no backlinks exist
            return results;
        }

        // Fallback to linear scan for tags and other complex cases
        self.backlinks_linear_scan(referenceable)
    }

    /// Graph-based backlink resolution for file-based referenceables.
    ///
    /// Traverses incoming edges to the target file to find all references
    /// pointing to it. Returns `Some(results)` for referenceables whose
    /// references create graph edges, `None` for types that need linear scan.
    ///
    /// # Which referenceables use graph-based resolution
    ///
    /// The graph captures file-to-file relationships where the reference type
    /// in `resolve_reference_target()` returns `Some(path)`:
    /// - `File` - referenced by `MDFileLink`, `{doc}` role
    /// - `Heading` - referenced by `MDHeadingLink`
    /// - `IndexedBlock` - referenced by `MDIndexedBlockLink`
    ///
    /// # Which referenceables need linear scan
    ///
    /// These referenceables are referenced by types that don't create edges:
    /// - `MystAnchor` - referenced by `{ref}` role (no edge)
    /// - `GlossaryTerm` - referenced by `{term}` role (no edge)
    /// - `MathLabel` - referenced by `{eq}` role (no edge)
    /// - `SubstitutionDef` - referenced by `Substitution` (file-local, no edge)
    /// - `Footnote` - referenced by `Footnote` ref (file-local, no edge)
    /// - `LinkRefDef` - referenced by `LinkRef` (file-local, no edge)
    /// - `Tag` - can be referenced from any file (no edge)
    fn backlinks_via_graph<'a>(
        &'a self,
        referenceable: &Referenceable,
    ) -> Option<Vec<(&'a Path, &'a Reference)>> {
        use petgraph::Direction;

        // Only handle referenceables whose references create graph edges
        // These correspond to MDFileLink, MDHeadingLink, MDIndexedBlockLink, {doc} role
        let target_path: &Path = match referenceable {
            // These create graph edges via resolve_reference_target()
            Referenceable::File(path, _)
            | Referenceable::Heading(path, _)
            | Referenceable::IndexedBlock(path, _) => path.as_path(),
            // These are referenced by types that don't create edges - use linear scan
            Referenceable::MystAnchor(..)
            | Referenceable::GlossaryTerm(..)
            | Referenceable::MathLabel(..)
            | Referenceable::SubstitutionDef(..)
            | Referenceable::Footnote(..)
            | Referenceable::LinkRefDef(..)
            | Referenceable::Tag(..)
            | Referenceable::UnresolvedFile(..)
            | Referenceable::UnresolvedHeading(..)
            | Referenceable::UnresolvedIndexedBlock(..) => return None,
        };

        // Get target node index
        let target_idx = self.node_index.get(target_path)?;

        // Collect backlinks by traversing incoming edges
        let mut results: Vec<(&Path, &Reference)> = self
            .graph
            .edges_directed(*target_idx, Direction::Incoming)
            .filter_map(|edge| {
                if let graph::EdgeKind::Reference { source_path, .. } = edge.weight() {
                    // Get the DocumentNode from graph for stable references
                    let source_node = self.get_document(source_path)?;

                    // Find references in this file that match the referenceable
                    // Note: A file may have multiple references to the same target
                    source_node
                        .references
                        .iter()
                        .find(|r| {
                            referenceable.matches_reference(
                                &self.root_dir,
                                r,
                                source_path.as_path(),
                            )
                        })
                        .map(|r| (source_path.as_path(), r))
                } else {
                    None
                }
            })
            .collect();

        // Sort by modification time (preserve existing behavior)
        results.sort_by(|a, b| {
            let time_a = std::fs::metadata(a.0).and_then(|m| m.modified()).ok();
            let time_b = std::fs::metadata(b.0).and_then(|m| m.modified()).ok();
            time_b.cmp(&time_a)
        });

        Some(results)
    }

    /// Linear scan fallback for backlink resolution.
    ///
    /// Used for tags (which span multiple files) and other edge cases.
    fn backlinks_linear_scan<'a>(
        &'a self,
        referenceable: &Referenceable,
    ) -> Vec<(&'a Path, &'a Reference)> {
        let references = self.select_references(None);

        references
            .into_par_iter()
            .filter(|(ref_path, reference)| {
                referenceable.matches_reference(&self.root_dir, reference, ref_path)
            })
            .map(|(path, reference)| {
                match std::fs::metadata(path).and_then(|meta| meta.modified()) {
                    Ok(modified) => (path, reference, modified),
                    Err(_) => (path, reference, SystemTime::UNIX_EPOCH),
                }
            })
            .collect::<Vec<_>>()
            .into_iter()
            .sorted_by_key(|(_, _, modified)| *modified)
            .rev()
            .map(|(one, two, _)| (one, two))
            .collect()
    }

    /// Resolves a reference to its target referenceables.
    ///
    /// Uses graph-based resolution for file-to-file references (O(degree) where
    /// degree = number of edges from source file), with fallback to linear scan
    /// for reference types not in the graph (tags, footnotes, etc.).
    pub fn select_referenceables_for_reference(
        &self,
        reference: &Reference,
        reference_path: &Path,
    ) -> Vec<Referenceable<'_>> {
        // Try graph-based resolution first for file-to-file references
        if let Some(results) = self.resolve_via_graph(reference, reference_path) {
            if !results.is_empty() {
                return results;
            }
        }

        // Fallback to linear scan for edge cases (tags, footnotes, unresolved refs)
        let referenceables = self.select_referenceable_nodes(None);

        referenceables
            .into_iter()
            .filter(|i| reference.references(self.root_dir(), reference_path, i))
            .collect()
    }

    /// Graph-based reference resolution for file-to-file relationships.
    ///
    /// Returns `Some(results)` if graph-based resolution is applicable and produces
    /// results, `None` if the reference type should use linear scan instead.
    fn resolve_via_graph<'a>(
        &'a self,
        reference: &Reference,
        reference_path: &Path,
    ) -> Option<Vec<Referenceable<'a>>> {
        // Get source node index
        let source_idx = self.node_index.get(reference_path)?;

        // Find matching outgoing edge
        for edge in self.graph.edges(*source_idx) {
            if let graph::EdgeKind::Reference {
                reference: edge_ref,
                ..
            } = edge.weight()
            {
                // Compare references - if they match, we found the edge
                if edge_ref == reference {
                    let target_idx = edge.target();
                    let target_node = &self.graph[target_idx];

                    // Get stable path reference from node_index
                    let path_ref = self.node_index.get_key_value(&target_node.path)?.0;

                    return Some(self.build_referenceable_for_reference(
                        reference,
                        path_ref,
                        target_node,
                    ));
                }
            }
        }

        // No matching edge found - reference might be unresolved or to non-file target
        None
    }

    /// Builds the appropriate Referenceable based on reference type.
    ///
    /// Given a resolved target file, constructs the correct Referenceable variant
    /// (File, Heading, IndexedBlock, etc.) based on what the reference points to.
    fn build_referenceable_for_reference<'a>(
        &'a self,
        reference: &Reference,
        path: &'a PathBuf,
        doc_node: &'a DocumentNode,
    ) -> Vec<Referenceable<'a>> {
        match reference {
            Reference::MDFileLink(_) => vec![Referenceable::File(path, doc_node)],
            Reference::MDHeadingLink(_, _, heading_ref) => doc_node
                .headings
                .iter()
                .find(|h| h.heading_text.to_lowercase() == heading_ref.to_lowercase())
                .map(|h| vec![Referenceable::Heading(path, h)])
                .unwrap_or_default(),
            Reference::MDIndexedBlockLink(_, _, block_ref) => doc_node
                .indexed_blocks
                .iter()
                .find(|b| b.index.to_lowercase() == block_ref.to_lowercase())
                .map(|b| vec![Referenceable::IndexedBlock(path, b)])
                .unwrap_or_default(),
            Reference::MystRole(_, MystRoleKind::Doc, _) => {
                vec![Referenceable::File(path, doc_node)]
            }
            // Other reference types don't resolve via graph
            _ => vec![],
        }
    }

    #[allow(deprecated)] // field deprecated has been deprecated in favor of using tags and will be removed in the future
    pub fn to_symbol_information(&self, referenceable: Referenceable) -> Option<SymbolInformation> {
        Some(SymbolInformation {
            name: referenceable.get_refname(self.root_dir())?.to_string(),
            kind: match referenceable {
                Referenceable::File(_, _) => SymbolKind::FILE,
                Referenceable::Tag(_, _) => SymbolKind::CONSTANT,
                _ => SymbolKind::KEY,
            },
            location: Location {
                uri: Url::from_file_path(referenceable.get_path()).ok()?,
                range: find_range(&referenceable)?,
            },
            container_name: None,
            tags: None,
            deprecated: None,
        })
    }

    /// Detect cycles in include directives.
    ///
    /// Include cycles cause infinite loops during Sphinx builds. This method
    /// uses Tarjan's strongly connected components algorithm to find all
    /// cycles in the include graph.
    ///
    /// # Returns
    ///
    /// A vector of cycles, where each cycle is a vector of file paths.
    /// Each cycle contains all files that form a circular include chain.
    /// Returns an empty vector if no cycles are detected.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // If a.md includes b.md, b.md includes c.md, c.md includes a.md:
    /// let cycles = vault.detect_include_cycles();
    /// assert_eq!(cycles.len(), 1);
    /// assert_eq!(cycles[0].len(), 3);
    /// ```
    #[allow(dead_code)] // Public API for consumers; not used internally yet
    pub fn detect_include_cycles(&self) -> Vec<Vec<PathBuf>> {
        use petgraph::algo::tarjan_scc;

        // Filter to only include edges
        let include_graph: petgraph::Graph<_, _, petgraph::Directed> = self.graph.filter_map(
            |_, node| Some(node.path.clone()),
            |_, edge| match edge {
                graph::EdgeKind::Include => Some(()),
                _ => None,
            },
        );

        // Find strongly connected components
        let sccs = tarjan_scc(&include_graph);

        // Filter to SCCs with more than 1 node (actual cycles)
        // Also include self-loops (single node with edge to itself)
        sccs.into_iter()
            .filter(|scc| {
                if scc.len() > 1 {
                    true
                } else if scc.len() == 1 {
                    // Check for self-loop
                    let idx = scc[0];
                    include_graph.contains_edge(idx, idx)
                } else {
                    false
                }
            })
            .map(|scc| {
                scc.into_iter()
                    .map(|idx| include_graph[idx].clone())
                    .collect()
            })
            .collect()
    }

    /// Find documents not reachable from the root via toctree edges.
    ///
    /// In Sphinx documentation, documents should be included in the toctree
    /// hierarchy. Documents not reachable from the root are considered "orphans"
    /// and will generate warnings during builds.
    ///
    /// # Arguments
    ///
    /// * `root` - Path to the root document (typically index.md)
    ///
    /// # Returns
    ///
    /// A vector of paths to orphan documents. Returns an empty vector if:
    /// - The root doesn't exist in the vault
    /// - All documents are reachable from the root
    ///
    /// # Example
    ///
    /// ```ignore
    /// let orphans = vault.find_orphan_documents(&vault_dir.join("index.md"));
    /// for orphan in orphans {
    ///     println!("Warning: {} is not in any toctree", orphan.display());
    /// }
    /// ```
    #[allow(dead_code)] // Public API for consumers; not used internally yet
    pub fn find_orphan_documents(&self, root: &Path) -> Vec<PathBuf> {
        use petgraph::visit::Bfs;
        use std::collections::HashSet;

        let root_idx = match self.node_index.get(root) {
            Some(&idx) => idx,
            None => return vec![], // No root = can't determine orphans
        };

        // Build a toctree-only graph for traversal
        let toctree_graph: petgraph::Graph<_, _, petgraph::Directed> = self.graph.filter_map(
            |idx, _| Some(idx),
            |_, edge| match edge {
                graph::EdgeKind::Toctree { .. } => Some(()),
                _ => None,
            },
        );

        // Map root_idx to the filtered graph's node index
        // Since filter_map preserves indices, root_idx should be valid
        let filtered_root = NodeIndex::new(root_idx.index());

        // BFS from root following Toctree edges only
        let mut reachable = HashSet::new();
        let mut bfs = Bfs::new(&toctree_graph, filtered_root);

        while let Some(node) = bfs.next(&toctree_graph) {
            // Map back to original graph index
            let original_idx = NodeIndex::new(node.index());
            reachable.insert(original_idx);
        }

        // All nodes not in reachable set are orphans
        self.graph
            .node_indices()
            .filter(|idx| !reachable.contains(idx))
            .map(|idx| self.graph[idx].path.clone())
            .collect()
    }

    /// Get all documents this file depends on (transitively).
    ///
    /// Uses depth-first search to find all documents reachable from the given
    /// file via any type of edge (references, toctree, include).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the source document
    ///
    /// # Returns
    ///
    /// A set of paths to all documents that `path` depends on, either directly
    /// or transitively. Returns an empty set if:
    /// - The path doesn't exist in the vault
    /// - The document has no outgoing references
    ///
    /// # Example
    ///
    /// ```ignore
    /// // If a.md links to b.md, and b.md links to c.md:
    /// let deps = vault.transitive_dependencies(&vault_dir.join("a.md"));
    /// assert!(deps.contains(&vault_dir.join("b.md")));
    /// assert!(deps.contains(&vault_dir.join("c.md")));
    /// ```
    #[allow(dead_code)] // Public API for consumers; not used internally yet
    pub fn transitive_dependencies(&self, path: &Path) -> HashSet<PathBuf> {
        use petgraph::visit::Dfs;

        let start_idx = match self.node_index.get(path) {
            Some(&idx) => idx,
            None => return HashSet::new(),
        };

        let mut deps = HashSet::new();
        let mut dfs = Dfs::new(&self.graph, start_idx);

        while let Some(node) = dfs.next(&self.graph) {
            if node != start_idx {
                deps.insert(self.graph[node].path.clone());
            }
        }

        deps
    }

    /// Get all documents that depend on this file (transitively).
    ///
    /// Uses depth-first search on the reversed graph to find all documents
    /// that can reach the given file via any type of edge.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the target document
    ///
    /// # Returns
    ///
    /// A set of paths to all documents that depend on `path`, either directly
    /// or transitively. Returns an empty set if:
    /// - The path doesn't exist in the vault
    /// - No documents reference this file
    ///
    /// # Example
    ///
    /// ```ignore
    /// // If a.md links to d.md, b.md links to d.md, c.md links to d.md:
    /// let dependents = vault.transitive_dependents(&vault_dir.join("d.md"));
    /// assert!(dependents.contains(&vault_dir.join("a.md")));
    /// assert!(dependents.contains(&vault_dir.join("b.md")));
    /// assert!(dependents.contains(&vault_dir.join("c.md")));
    /// ```
    #[allow(dead_code)] // Public API for consumers; not used internally yet
    pub fn transitive_dependents(&self, path: &Path) -> HashSet<PathBuf> {
        use petgraph::visit::{Dfs, Reversed};

        let start_idx = match self.node_index.get(path) {
            Some(&idx) => idx,
            None => return HashSet::new(),
        };

        // Use reversed graph to traverse incoming edges
        let reversed = Reversed(&self.graph);
        let mut dependents = HashSet::new();
        let mut dfs = Dfs::new(&reversed, start_idx);

        while let Some(node) = dfs.next(&reversed) {
            if node != start_idx {
                dependents.insert(self.graph[node].path.clone());
            }
        }

        dependents
    }
}

pub enum Preview {
    Text(String),

    Empty,
}

impl From<String> for Preview {
    fn from(value: String) -> Self {
        Preview::Text(value)
    }
}

use Preview::*;

impl Vault {
    pub fn select_referenceable_preview(&self, referenceable: &Referenceable) -> Option<Preview> {
        if self
            .ropes
            .get(referenceable.get_path())
            .is_some_and(|rope| rope.len_lines() == 1)
        {
            return Some(Empty);
        }

        match referenceable {
            Referenceable::Footnote(_, _) | Referenceable::LinkRefDef(..) => {
                let range = referenceable.get_range()?;
                Some(
                    String::from_iter(
                        self.select_line(referenceable.get_path(), range.start.line as isize)?,
                    )
                    .into(),
                )
            }
            Referenceable::Heading(_, _) => {
                let range = referenceable.get_range()?;
                Some(
                    (range.start.line..=range.end.line + 10)
                        .filter_map(|ln| self.select_line(referenceable.get_path(), ln as isize)) // flatten those options!
                        .map(String::from_iter)
                        .join("")
                        .into(),
                )
            }
            Referenceable::IndexedBlock(_, _) => {
                let range = referenceable.get_range()?;
                self.select_line(referenceable.get_path(), range.start.line as isize)
                    .map(String::from_iter)
                    .map(Into::into)
            }
            Referenceable::File(_, _) => {
                Some(
                    (0..=13)
                        .filter_map(|ln| self.select_line(referenceable.get_path(), ln as isize)) // flatten those options!
                        .map(String::from_iter)
                        .join("")
                        .into(),
                )
            }
            Referenceable::Tag(_, _) => None,
            Referenceable::UnresolvedFile(_, _) => None,
            Referenceable::UnresolvedHeading(_, _, _) => None,
            Referenceable::UnresolvedIndexedBlock(_, _, _) => None,
            Referenceable::MystAnchor(path, symbol) => {
                // Show the line where the anchor is defined
                self.select_line(path, symbol.line as isize)
                    .map(String::from_iter)
                    .map(Into::into)
            }
            Referenceable::GlossaryTerm(path, term) => {
                // Show the term name and definition preview
                self.select_line(path, term.range.start.line as isize)
                    .map(String::from_iter)
                    .map(Into::into)
            }
            Referenceable::MathLabel(path, symbol) => {
                // Show the line where the math directive is defined
                self.select_line(path, symbol.line as isize)
                    .map(String::from_iter)
                    .map(Into::into)
            }
            Referenceable::SubstitutionDef(_, sub_def) => {
                // Show the substitution name and value
                Some(format!("{}: {}", sub_def.name, sub_def.value).into())
            }
        }
    }

    pub fn select_blocks(&self) -> Vec<Block<'_>> {
        self.ropes
            .par_iter()
            .map(|(path, rope)| {
                rope.lines()
                    .enumerate()
                    .flat_map(|(i, line)| {
                        let string = line.as_str()?;

                        Some(Block {
                            text: string.trim(),
                            range: MyRange(tower_lsp::lsp_types::Range {
                                start: Position {
                                    line: i as u32,
                                    character: 0,
                                },
                                end: Position {
                                    line: i as u32,
                                    character: string.len() as u32,
                                },
                            }),
                            file: path,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .flatten()
            .filter(|block| !block.text.is_empty())
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Copy)]
pub struct Block<'a> {
    pub text: &'a str,
    pub range: MyRange,
    pub file: &'a Path,
}

impl AsRef<str> for Block<'_> {
    fn as_ref(&self) -> &str {
        self.text
    }
}

impl Rangeable for Reference {
    fn range(&self) -> &MyRange {
        &self.range
    }
}

#[derive(Debug, PartialEq, Eq, Default, Hash, Clone)]
pub struct MDFile {
    pub references: Vec<Reference>,
    pub headings: Vec<MDHeading>,
    pub indexed_blocks: Vec<MDIndexedBlock>,
    pub tags: Vec<MDTag>,
    pub footnotes: Vec<MDFootnote>,
    pub path: PathBuf,
    pub link_reference_definitions: Vec<MDLinkReferenceDefinition>,
    pub metadata: Option<MDMetadata>,
    pub codeblocks: Vec<MDCodeBlock>,
    pub myst_symbols: Vec<MystSymbol>,
    /// Glossary terms extracted from `{glossary}` directives
    pub glossary_terms: Vec<GlossaryTerm>,
    /// Substitution definitions from frontmatter
    pub substitution_defs: Vec<MDSubstitutionDef>,
}

impl MDFile {
    fn new(context: &Settings, text: &str, path: PathBuf) -> MDFile {
        let myst_symbols = myst_parser::parse(text);
        let glossary_terms = myst_parser::parse_glossary_terms(text);
        let code_blocks = MDCodeBlock::new(text).collect_vec();
        let file_name = path
            .file_stem()
            .expect("file should have file stem")
            .to_str()
            .unwrap_or_default();
        let links = match context {
            Settings {
                references_in_codeblocks: false,
                ..
            } => Reference::new(text, file_name)
                .filter(|it| !code_blocks.iter().any(|codeblock| codeblock.includes(it)))
                .collect_vec(),
            _ => Reference::new(text, file_name).collect_vec(),
        };
        let headings = MDHeading::new(text)
            .filter(|it| !code_blocks.iter().any(|codeblock| codeblock.includes(it)));
        let footnotes = MDFootnote::new(text)
            .filter(|it| !code_blocks.iter().any(|codeblock| codeblock.includes(it)));
        let link_refs = MDLinkReferenceDefinition::new(text)
            .filter(|it| !code_blocks.iter().any(|codeblock| codeblock.includes(it)));
        let indexed_blocks = MDIndexedBlock::new(text)
            .filter(|it| !code_blocks.iter().any(|codeblock| codeblock.includes(it)));
        let tags = match context {
            Settings {
                tags_in_codeblocks: false,
                ..
            } => MDTag::new(text)
                .filter(|it| !code_blocks.iter().any(|codeblock| codeblock.includes(it)))
                .collect_vec(),
            _ => MDTag::new(text).collect_vec(),
        };
        let metadata = MDMetadata::new(text);

        // Extract substitution definitions from metadata
        let substitution_defs = metadata
            .as_ref()
            .map(|m| {
                m.substitutions()
                    .iter()
                    .map(|(name, value)| MDSubstitutionDef {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        MDFile {
            references: links,
            headings: headings.collect(),
            indexed_blocks: indexed_blocks.collect(),
            tags,
            footnotes: footnotes.collect(),
            path,
            link_reference_definitions: link_refs.collect(),
            metadata,
            codeblocks: code_blocks,
            myst_symbols,
            glossary_terms,
            substitution_defs,
        }
    }

    #[allow(dead_code)] // Used via From<&MDFile> for DocumentNode
    pub fn file_name(&self) -> Option<&str> {
        self.path.file_stem()?.to_str()
    }
}

// Note: MDFile::get_referenceables() has been moved to DocumentNode::get_referenceables()
// as part of the graph migration (Phase 5). DocumentNode is now the canonical source
// for document data, with references stored as graph edges.

#[derive(Debug, PartialEq, Eq, Default, Clone, Hash)]
pub struct ReferenceData {
    pub reference_text: String,
    pub display_text: Option<String>,
    pub range: MyRange,
}

type File = String;
type Specialref = String;

// TODO: I should probably make this my own hash trait so it is more clear what it does
impl Hash for Reference {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.data().reference_text.hash(state)
    }
}

/// MyST role kind for inline references like {ref}`target` or {doc}`path`
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MystRoleKind {
    Ref,      // {ref}`target` - cross-reference to anchor
    NumRef,   // {numref}`Figure %s <target>` - numbered reference
    Eq,       // {eq}`equation-label` - equation reference
    Doc,      // {doc}`/path/to/file` - document link
    Download, // {download}`file.zip` - downloadable file
    Term,     // {term}`glossary-term` - glossary reference
    Abbr,     // {abbr}`MyST (Markedly Structured Text)` - abbreviation
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Reference {
    #[allow(dead_code)] // Tag support; matched but not currently constructed
    Tag(ReferenceData),
    MDFileLink(ReferenceData),
    MDHeadingLink(ReferenceData, File, Specialref),
    MDIndexedBlockLink(ReferenceData, File, Specialref),
    Footnote(ReferenceData),
    LinkRef(ReferenceData),
    /// MyST role reference: {role}`target`
    /// Fields: (data, role_kind, target)
    MystRole(ReferenceData, MystRoleKind, String),
    /// Image link: ![alt](path)
    /// reference_text contains the image path
    ImageLink(ReferenceData),
    /// MyST substitution reference: {{variable}}
    /// reference_text contains the variable name (without braces)
    Substitution(ReferenceData),
}

impl Deref for Reference {
    type Target = ReferenceData;
    fn deref(&self) -> &Self::Target {
        self.data()
    }
}

impl Default for Reference {
    fn default() -> Self {
        MDFileLink(ReferenceData::default())
    }
}

use Reference::*;

use crate::config::Settings;
use crate::myst_parser::{self, GlossaryTerm, MystSymbol, MystSymbolKind};

use self::{metadata::MDMetadata, parsing::MDCodeBlock};

/// Trait for common operations on Reference types.
///
/// This trait centralizes operations that would otherwise require exhaustive
/// pattern matching across all Reference variants. When adding a new Reference
/// variant, implement these methods once rather than updating 30+ match sites.
///
/// # Design Rationale
///
/// The Reference enum has 9 variants, and operations on references were scattered
/// across multiple files with exhaustive matches. This trait provides:
///
/// 1. **Single point of implementation** - New variants only need trait impl
/// 2. **Polymorphic dispatch** - Call sites use methods instead of matches
/// 3. **Self-documenting** - Trait methods describe reference capabilities
///
/// # Example
///
/// ```rust,ignore
/// // Before: scattered match statements
/// match reference {
///     Reference::Tag(data) => format!("tag diagnostic"),
///     Reference::MDFileLink(data) => format!("file diagnostic"),
///     // ... 7 more arms
/// }
///
/// // After: single method call
/// let message = reference.generate_diagnostic_message(usage_count);
/// ```
#[allow(dead_code)] // Trait defined for extensibility; methods used via inherent impl
pub trait ReferenceOps {
    /// Get the underlying reference data (range, reference_text, display_text).
    fn data(&self) -> &ReferenceData;

    /// Returns a static string identifying the reference type.
    ///
    /// Useful for diagnostics, logging, and debugging.
    fn reference_type_name(&self) -> &'static str;

    /// Returns whether this reference type supports hover preview display.
    ///
    /// Tags, image links, and substitutions don't resolve to markdown
    /// referenceables that have preview content.
    fn has_preview(&self) -> bool;

    /// Generate a diagnostic message for an unresolved reference.
    ///
    /// Provides type-specific messages to help users understand exactly what
    /// type of target is missing.
    fn generate_diagnostic_message(&self, usage_count: usize) -> String;

    /// Generate the replacement text for a reference when its target is renamed.
    ///
    /// This method encapsulates the logic for formatting reference text during
    /// rename operations. Returns `Some(new_text)` if this reference type should
    /// be updated when the given referenceable is renamed, or `None` if this
    /// reference type doesn't apply to the referenceable being renamed.
    ///
    /// # Arguments
    /// * `referenceable` - The target being renamed
    /// * `new_ref_name` - The new name for the target
    /// * `root_dir` - The vault root directory (for path resolution)
    ///
    /// # Returns
    /// `Some(String)` with the new reference text, or `None` if not applicable
    fn get_rename_text(
        &self,
        referenceable: &Referenceable,
        new_ref_name: &str,
        root_dir: &Path,
    ) -> Option<String>;
}

impl ReferenceOps for Reference {
    fn data(&self) -> &ReferenceData {
        Reference::data(self)
    }

    fn reference_type_name(&self) -> &'static str {
        Reference::reference_type_name(self)
    }

    fn has_preview(&self) -> bool {
        Reference::has_preview(self)
    }

    fn generate_diagnostic_message(&self, usage_count: usize) -> String {
        Reference::generate_diagnostic_message(self, usage_count)
    }

    fn get_rename_text(
        &self,
        referenceable: &Referenceable,
        new_ref_name: &str,
        root_dir: &Path,
    ) -> Option<String> {
        Reference::get_rename_text(self, referenceable, new_ref_name, root_dir)
    }
}

impl Reference {
    pub fn data(&self) -> &ReferenceData {
        match &self {
            Tag(data, ..) => data,
            Footnote(data) => data,
            MDFileLink(data, ..) => data,
            MDHeadingLink(data, ..) => data,
            MDIndexedBlockLink(data, ..) => data,
            LinkRef(data, ..) => data,
            MystRole(data, ..) => data,
            ImageLink(data) => data,
            Substitution(data) => data,
        }
    }

    pub fn matches_type(&self, other: &Reference) -> bool {
        match &other {
            Tag(..) => matches!(self, Tag(..)),
            Footnote(..) => matches!(self, Footnote(..)),
            MDFileLink(..) => matches!(self, MDFileLink(..)),
            MDHeadingLink(..) => matches!(self, MDHeadingLink(..)),
            MDIndexedBlockLink(..) => matches!(self, MDIndexedBlockLink(..)),
            LinkRef(..) => matches!(self, LinkRef(..)),
            MystRole(..) => matches!(self, MystRole(..)),
            ImageLink(..) => matches!(self, ImageLink(..)),
            Substitution(..) => matches!(self, Substitution(..)),
        }
    }

    /// Returns a static string identifying the reference type.
    ///
    /// This is useful for diagnostics, logging, and debugging without needing
    /// to match on all variants.
    #[allow(dead_code)] // Part of ReferenceOps trait, used for extensibility
    pub fn reference_type_name(&self) -> &'static str {
        match self {
            Tag(..) => "tag",
            Footnote(..) => "footnote",
            MDFileLink(..) => "file_link",
            MDHeadingLink(..) => "heading_link",
            MDIndexedBlockLink(..) => "indexed_block_link",
            LinkRef(..) => "link_ref",
            MystRole(..) => "myst_role",
            ImageLink(..) => "image_link",
            Substitution(..) => "substitution",
        }
    }

    /// Returns whether this reference type supports hover preview display.
    ///
    /// Tags, image links, and substitutions don't resolve to markdown
    /// referenceables that have preview content.
    pub fn has_preview(&self) -> bool {
        match self {
            Tag(_) => false,
            ImageLink(_) => false,
            Substitution(_) => false,
            // All other reference types support preview
            Footnote(_)
            | MDFileLink(..)
            | MDHeadingLink(..)
            | MDIndexedBlockLink(..)
            | LinkRef(..)
            | MystRole(..) => true,
        }
    }

    /// Generate a diagnostic message for an unresolved reference.
    ///
    /// Provides type-specific messages to help users understand exactly what
    /// type of target is missing. Appends usage count if > 1.
    ///
    /// # Arguments
    /// * `usage_count` - Number of times this reference appears in the vault
    pub fn generate_diagnostic_message(&self, usage_count: usize) -> String {
        let base_message = match self {
            MystRole(_, kind, target) => match kind {
                MystRoleKind::Ref | MystRoleKind::NumRef => {
                    format!("Unresolved reference to anchor '{}'", target)
                }
                MystRoleKind::Doc => {
                    format!("Unresolved document reference '{}'", target)
                }
                MystRoleKind::Download => {
                    format!("Unresolved download reference '{}'", target)
                }
                MystRoleKind::Term => {
                    format!("Unresolved glossary term '{}'", target)
                }
                MystRoleKind::Eq => {
                    format!("Unresolved equation reference '{}'", target)
                }
                MystRoleKind::Abbr => {
                    // Abbreviations don't reference external targets
                    "Unresolved Reference".to_string()
                }
            },
            ImageLink(data) => {
                format!("Missing image file '{}'", data.reference_text)
            }
            Substitution(data) => {
                // Note: {{{{ produces {{ and }}}} produces }} in the output
                format!("Undefined substitution '{{{{{}}}}}'", data.reference_text)
            }
            _ => "Unresolved Reference".to_string(),
        };

        // Append usage count if the reference appears multiple times
        if usage_count > 1 {
            format!("{} (used {} times)", base_message, usage_count)
        } else {
            base_message
        }
    }

    /// Generate the replacement text for a reference when its target is renamed.
    ///
    /// This consolidates the rename logic that was previously scattered in rename.rs.
    /// Each reference type knows how to format itself for different referenceable types.
    ///
    /// # Arguments
    /// * `referenceable` - The target being renamed
    /// * `new_ref_name` - The new name for the target
    /// * `root_dir` - The vault root directory (for path resolution)
    ///
    /// # Returns
    /// `Some(String)` with the new reference text, or `None` if not applicable
    pub fn get_rename_text(
        &self,
        referenceable: &Referenceable,
        new_ref_name: &str,
        root_dir: &Path,
    ) -> Option<String> {
        match self {
            Tag(data) => {
                // Tags can only be renamed when the referenceable is a Tag
                if !matches!(referenceable, Referenceable::Tag(..)) {
                    return None;
                }
                let old_refname = referenceable.get_refname(root_dir)?;
                let new_text = format!(
                    "#{}",
                    data.reference_text.replacen(&*old_refname, new_ref_name, 1)
                );
                Some(new_text)
            }
            MDFileLink(data) => {
                // File links only update when renaming Files
                if !matches!(referenceable, Referenceable::File(..)) {
                    return None;
                }
                let display_part = data
                    .display_text
                    .as_ref()
                    .map(|text| format!("|{text}"))
                    .unwrap_or_default();
                Some(format!("[{}]({})", display_part, new_ref_name))
            }
            MDHeadingLink(data, _file, infile) | MDIndexedBlockLink(data, _file, infile) => {
                match referenceable {
                    Referenceable::File(..) => {
                        // When renaming a file, update the file part but keep the infile reference
                        let display_part = data
                            .display_text
                            .as_ref()
                            .map(|text| format!("|{text}"))
                            .unwrap_or_default();
                        Some(format!("[{}]({}#{})", display_part, new_ref_name, infile))
                    }
                    Referenceable::Heading(..) if matches!(self, MDHeadingLink(..)) => {
                        // When renaming a heading, update to the new heading name
                        let display_part = data
                            .display_text
                            .as_ref()
                            .map(|text| format!("|{text}"))
                            .unwrap_or_default();
                        Some(format!("[{}]({})", display_part, new_ref_name))
                    }
                    _ => None,
                }
            }
            MystRole(data, role_kind, _old_target) => {
                // MyST roles only update when renaming MystAnchors, and only for Ref/NumRef roles
                if !matches!(referenceable, Referenceable::MystAnchor(..)) {
                    return None;
                }
                if !matches!(role_kind, MystRoleKind::Ref | MystRoleKind::NumRef) {
                    return None;
                }
                let role_name = match role_kind {
                    MystRoleKind::Ref => "ref",
                    MystRoleKind::NumRef => "numref",
                    _ => return None, // Shouldn't happen due to guard above
                };
                let new_text = match &data.display_text {
                    Some(display) => {
                        // Format: {role}`display text <new-target>`
                        format!("{{{}}}`{} <{}>`", role_name, display, new_ref_name)
                    }
                    None => {
                        // Format: {role}`new-target`
                        format!("{{{}}}`{}`", role_name, new_ref_name)
                    }
                };
                Some(new_text)
            }
            // These reference types don't participate in rename operations
            Footnote(..) | LinkRef(..) | ImageLink(..) | Substitution(..) => None,
        }
    }

    /// AST-based reference parsing using markdown-rs.
    ///
    /// This method uses the markdown-rs AST parser to extract references.
    /// It extracts MD links, footnotes, and link references but does NOT extract tags
    /// (tags are extracted separately by MDTag::new()).
    ///
    /// # Arguments
    /// * `text` - The markdown source text
    /// * `file_name` - The name of the current file (used for same-file references like `#heading`)
    ///
    /// # Returns
    /// An iterator over all `Reference` items found in the document (excluding tags).
    pub fn new(text: &str, file_name: &str) -> impl Iterator<Item = Reference> {
        ast_refs::extract_references_from_ast(text, file_name).into_iter()
    }

    pub fn references(
        &self,
        root_dir: &Path,
        file_path: &Path,
        referenceable: &Referenceable,
    ) -> bool {
        let text = &self.data().reference_text;
        match referenceable {
            &Referenceable::Tag(_, _) => match self {
                Tag(..) => {
                    referenceable
                        .get_refname(root_dir)
                        .map(|thing| thing.to_string())
                        == Some(text.to_string())
                }
                MDFileLink(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
                MystRole(..) => false,
                ImageLink(_) => false, // Images don't reference markdown content
                Substitution(_) => false, // Substitutions don't reference tags
            },
            &Referenceable::Footnote(path, _footnote) => match self {
                Footnote(..) => {
                    referenceable.get_refname(root_dir).as_deref() == Some(text)
                        && path.as_path() == file_path
                }
                Tag(_) => false,
                MDFileLink(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                LinkRef(_) => false,
                MystRole(..) => false,
                ImageLink(_) => false,
                Substitution(_) => false, // Substitutions don't reference footnotes
            },
            &Referenceable::File(..) | &Referenceable::UnresolvedFile(..) => match self {
                MDFileLink(ReferenceData {
                    reference_text: file_ref_text,
                    ..
                }) => matches_path_or_file(file_ref_text, referenceable.get_refname(root_dir)),
                // {doc}`path` role references files
                MystRole(_, MystRoleKind::Doc, target) => {
                    matches_path_or_file(target, referenceable.get_refname(root_dir))
                }
                Tag(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
                MystRole(..) => false, // Other role types don't reference files
                ImageLink(_) => false, // Images reference files on disk, not markdown referenceables
                Substitution(_) => false, // Substitutions don't reference files
            },
            &Referenceable::Heading(
                ..,
                MDHeading {
                    heading_text: infile_ref,
                    ..
                },
            )
            | &Referenceable::UnresolvedHeading(.., infile_ref)
            | &Referenceable::IndexedBlock(
                ..,
                MDIndexedBlock {
                    index: infile_ref, ..
                },
            )
            | &Referenceable::UnresolvedIndexedBlock(.., infile_ref) => match self {
                MDHeadingLink(.., file_ref_text, link_infile_ref)
                | MDIndexedBlockLink(.., file_ref_text, link_infile_ref) => {
                    matches_path_or_file(file_ref_text, referenceable.get_refname(root_dir))
                        && link_infile_ref.to_lowercase() == infile_ref.to_lowercase()
                }
                // {ref}`target` can reference headings
                MystRole(_, MystRoleKind::Ref, target) => {
                    target.to_lowercase() == infile_ref.to_lowercase()
                }
                Tag(_) => false,
                MDFileLink(_) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
                MystRole(..) => false, // Other role types don't reference headings
                ImageLink(_) => false,
                Substitution(_) => false, // Substitutions don't reference headings
            },
            Referenceable::LinkRefDef(path, _link_ref) => match self {
                Tag(_) => false,
                MDFileLink(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                Footnote(_) => false,
                LinkRef(data) => {
                    Some(data.reference_text.to_lowercase())
                        == referenceable
                            .get_refname(root_dir)
                            .as_deref()
                            .map(|string| string.to_lowercase())
                        && file_path == *path
                }
                MystRole(..) => false,
                ImageLink(_) => false,
                Substitution(_) => false, // Substitutions don't reference link defs
            },
            Referenceable::MystAnchor(_, symbol) => match self {
                // {ref}`target` and {numref}`target` roles can reference MyST anchors
                MystRole(_, MystRoleKind::Ref, target)
                | MystRole(_, MystRoleKind::NumRef, target) => {
                    target.to_lowercase() == symbol.name.to_lowercase()
                }
                Tag(_) => false,
                MDFileLink(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
                MystRole(..) => false, // Other role types don't reference anchors
                ImageLink(_) => false,
                Substitution(_) => false, // Substitutions don't reference anchors
            },
            Referenceable::GlossaryTerm(_, glossary_term) => match self {
                // {term}`term-name` roles reference glossary terms
                MystRole(_, MystRoleKind::Term, target) => {
                    target.to_lowercase() == glossary_term.term.to_lowercase()
                }
                Tag(_) => false,
                MDFileLink(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
                MystRole(..) => false, // Other role types don't reference glossary terms
                ImageLink(_) => false,
                Substitution(_) => false, // Substitutions don't reference glossary terms
            },
            Referenceable::MathLabel(_, symbol) => match self {
                // {eq}`label` roles reference math equation labels
                MystRole(_, MystRoleKind::Eq, target) => symbol
                    .label
                    .as_ref()
                    .is_some_and(|label| target.to_lowercase() == label.to_lowercase()),
                Tag(_) => false,
                MDFileLink(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
                MystRole(..) => false, // Other role types don't reference math labels
                ImageLink(_) => false,
                Substitution(_) => false, // Substitutions don't reference math labels
            },
            // SubstitutionDef: {{variable}} references file-local definitions
            // IMPORTANT: Substitutions are FILE-LOCAL only
            Referenceable::SubstitutionDef(def_path, sub_def) => match self {
                Substitution(data) => {
                    // Must be in the same file AND have matching name
                    file_path == def_path.as_path()
                        && data.reference_text.to_lowercase() == sub_def.name.to_lowercase()
                }
                Tag(_) => false,
                MDFileLink(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
                MystRole(..) => false,
                ImageLink(_) => false,
            },
        }
    }
}

impl MDHeading {
    fn new(text: &str) -> impl Iterator<Item = MDHeading> + '_ {
        static HEADING_RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(?<starter>#+) (?<heading_text>.+)").unwrap());

        let headings = HEADING_RE
            .captures_iter(text)
            .flat_map(
                |c| match (c.get(0), c.name("heading_text"), c.name("starter")) {
                    (Some(full), Some(text), Some(starter)) => Some((full, text, starter)),
                    _ => None,
                },
            )
            .map(|(full_heading, heading_match, starter)| MDHeading {
                heading_text: heading_match.as_str().trim_end().into(),
                range: MyRange::from_range(&Rope::from_str(text), full_heading.range()),
                level: HeadingLevel(starter.as_str().len()),
            });

        headings
    }
}

impl MDIndexedBlock {
    fn new(text: &str) -> impl Iterator<Item = MDIndexedBlock> + '_ {
        static INDEXED_BLOCK_RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r".+ (\^(?<index>\w+))").unwrap());

        let indexed_blocks = INDEXED_BLOCK_RE
            .captures_iter(text)
            .flat_map(|c| match (c.get(1), c.name("index")) {
                (Some(full), Some(index)) => Some((full, index)),
                _ => None,
            })
            .map(|(full, index)| MDIndexedBlock {
                index: index.as_str().into(),
                range: MyRange::from_range(&Rope::from_str(text), full.range()),
            });

        indexed_blocks
    } // Make this better identify the full blocks
}

impl MDFootnote {
    fn new(text: &str) -> impl Iterator<Item = MDFootnote> + '_ {
        // static FOOTNOTE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r".+ (\^(?<index>\w+))").unwrap());
        static FOOTNOTE_RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"\[(?<index>\^[^ \[\]]+)\]\:(?<text>.+)").unwrap());

        let footnotes = FOOTNOTE_RE
            .captures_iter(text)
            .flat_map(|c| match (c.get(0), c.name("index"), c.name("text")) {
                (Some(full), Some(index), Some(footnote_text)) => {
                    Some((full, index, footnote_text))
                }
                _ => None,
            })
            .map(|(full, index, footnote_text)| MDFootnote {
                footnote_text: footnote_text.as_str().trim_start().into(),
                index: index.as_str().into(),
                range: MyRange::from_range(&Rope::from_str(text), full.range()),
            });

        footnotes
    }
}

impl MDTag {
    fn new(text: &str) -> impl Iterator<Item = MDTag> + '_ {
        static TAG_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r#"(?x)
                # 1. Boundary assertion: The tag must be preceded by the start of the string, a newline, or whitespace.
                #    Using a non-capturing group (?:...) for efficiency.
                (?: \A | \n | \s )
        
                # 2. <full> capturing group: Captures the entire tag, including the '#' character.
                (?<full>
                    \#          # Matches the literal '#' character.
                    
                    # 3. <tag> capturing group: Captures the actual content of the tag.
                    (?<tag>
                        # First character of the tag:
                        # Cannot be a digit. Can be letters (Unicode), underscore, hyphen, slash, or various quotes.
                        [\p{L}_/'"‘’“”-]
        
                        # Subsequent characters of the tag:
                        # Can be letters (Unicode), digits, underscore, hyphen, slash, or various quotes.
                        [\p{L}0-9_/'"‘’“”-]*
                    )
                )
    "#).unwrap()
        });

        let tagged_blocks = TAG_RE
            .captures_iter(text)
            .flat_map(|c| match (c.name("full"), c.name("tag")) {
                (Some(full), Some(index)) => Some((full, index)),
                _ => None,
            })
            .filter(|(_, index)| index.as_str().chars().any(|c| c.is_alphabetic()))
            .map(|(full, index)| MDTag {
                tag_ref: index.as_str().into(),
                range: MyRange::from_range(&Rope::from_str(text), full.range()),
            });

        tagged_blocks
    }
}

impl MDLinkReferenceDefinition {
    fn new(text: &str) -> impl Iterator<Item = MDLinkReferenceDefinition> + '_ {
        static REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"\[(?<index>[^\^][^ \[\]]+)\]\:(?<text>.+)").unwrap());

        let result = REGEX
            .captures_iter(text)
            .flat_map(|c| match (c.get(0), c.name("index"), c.name("text")) {
                (Some(full), Some(index), Some(text)) => Some((full, index, text)),
                _ => None,
            })
            .flat_map(|(full, index, url)| {
                Some(MDLinkReferenceDefinition {
                    link_ref_name: index.as_str().to_string(),
                    range: MyRange::from_range(&Rope::from_str(text), full.range()),
                    url: url.as_str().trim().to_string(),
                    title: None,
                })
            });

        result
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
/**
An algebraic type for methods on all referenceables, which are anything able to be referenced through a Markdown link or tag.
These include files, headings, indexed blocks, tags, MyST anchors, etc.

I chose to use an enum instead of a trait as (1) I dislike the ergonomics with dynamic dispatch, (2) it is sometimes necessary to differentiate between members of this abstraction, (3) it was convenient for this abstraction to hold the path of the referenceable for use in matching link names etc...

The vault struct is focused on presenting data from the vault through a good usable interface. The vault module as a whole, however, is in charge of interfacing with the Markdown/MyST syntax, which is where the methods on this enum are applicable. Markdown has specific linking styles, and the methods on this enum provide a way to work with this syntax in a way that decouples the interpretation from other modules. The most common method is `is_reference` which tells if a piece of text is a reference to a particular referenceable (which is implemented differently for each type of referenceable). As a whole, this provides an abstraction around interpreting Markdown/MyST syntax.
*/
pub enum Referenceable<'a> {
    File(&'a PathBuf, &'a DocumentNode),
    Heading(&'a PathBuf, &'a MDHeading),
    IndexedBlock(&'a PathBuf, &'a MDIndexedBlock),
    Tag(&'a PathBuf, &'a MDTag),
    Footnote(&'a PathBuf, &'a MDFootnote),
    // TODO: Get rid of useless path here
    UnresolvedFile(PathBuf, &'a String),
    UnresolvedHeading(PathBuf, &'a String, &'a String),
    /// full path, link path, index (without ^)
    UnresolvedIndexedBlock(PathBuf, &'a String, &'a String),
    LinkRefDef(&'a PathBuf, &'a MDLinkReferenceDefinition),
    /// MyST anchor target: `(my-target)=`
    MystAnchor(&'a PathBuf, &'a MystSymbol),
    /// Glossary term: term name referenced via `{term}`term``
    GlossaryTerm(&'a PathBuf, &'a GlossaryTerm),
    /// Math equation label: referenced via `{eq}`label``
    /// Only created for `{math}` directives that have a `:label:` option
    MathLabel(&'a PathBuf, &'a MystSymbol),
    /// Substitution definition from frontmatter: {{variable}}
    /// **File-local**: substitutions only resolve within the same file
    SubstitutionDef(&'a PathBuf, &'a MDSubstitutionDef),
}
impl Referenceable<'_> {
    /// Gets the generic reference name for a referenceable. This will not include any display text. If trying to determine if text is a reference of a particular referenceable, use the `is_reference` function
    pub fn get_refname(&self, root_dir: &Path) -> Option<Refname> {
        match self {
            Referenceable::File(path, _) => {
                get_relative_ref_path(root_dir, path).map(|string| Refname {
                    full_refname: string.to_owned(),
                    path: string.to_owned().into(),
                    ..Default::default()
                })
            }

            Referenceable::Heading(path, heading) => get_relative_ref_path(root_dir, path)
                .map(|refpath| {
                    (
                        refpath.clone(),
                        format!("{}#{}", refpath, heading.heading_text),
                    )
                })
                .map(|(path, full_refname)| Refname {
                    full_refname,
                    path: path.into(),
                    infile_ref: <std::string::String as Clone>::clone(&heading.heading_text).into(),
                }),

            Referenceable::IndexedBlock(path, index) => get_relative_ref_path(root_dir, path)
                .map(|refpath| (refpath.clone(), format!("{}#^{}", refpath, index.index)))
                .map(|(path, full_refname)| Refname {
                    full_refname,
                    path: path.into(),
                    infile_ref: format!("^{}", index.index).into(),
                }),

            Referenceable::Tag(_, tag) => Some(Refname {
                full_refname: format!("#{}", tag.tag_ref),
                path: Some(tag.tag_ref.clone()),
                infile_ref: None,
            }),

            Referenceable::Footnote(_, footnote) => Some(footnote.index.clone().into()),

            Referenceable::UnresolvedHeading(_, path, heading) => {
                Some(format!("{}#{}", path, heading)).map(|full_ref| Refname {
                    full_refname: full_ref,
                    path: path.to_string().into(),
                    infile_ref: heading.to_string().into(),
                })
            }

            Referenceable::UnresolvedFile(_, path) => Some(Refname {
                full_refname: path.to_string(),
                path: Some(path.to_string()),
                ..Default::default()
            }),

            Referenceable::UnresolvedIndexedBlock(_, path, index) => {
                Some(format!("{}#^{}", path, index)).map(|full_ref| Refname {
                    full_refname: full_ref,
                    path: path.to_string().into(),
                    infile_ref: format!("^{}", index).into(),
                })
            }
            Referenceable::LinkRefDef(_, refdef) => Some(Refname {
                full_refname: refdef.link_ref_name.clone(),
                infile_ref: None,
                path: None,
            }),
            Referenceable::MystAnchor(_, symbol) => Some(Refname {
                full_refname: symbol.name.clone(),
                infile_ref: Some(symbol.name.clone()),
                path: None,
            }),
            Referenceable::GlossaryTerm(_, glossary_term) => Some(Refname {
                full_refname: glossary_term.term.clone(),
                infile_ref: Some(glossary_term.term.clone()),
                path: None,
            }),
            Referenceable::MathLabel(_, symbol) => {
                // MathLabel uses the label field from the MystSymbol
                symbol.label.as_ref().map(|label| Refname {
                    full_refname: label.clone(),
                    infile_ref: Some(label.clone()),
                    path: None,
                })
            }
            Referenceable::SubstitutionDef(_, sub_def) => Some(Refname {
                full_refname: sub_def.name.clone(),
                infile_ref: Some(sub_def.name.clone()),
                path: None,
            }),
        }
    }

    pub fn matches_reference(
        &self,
        root_dir: &Path,
        reference: &Reference,
        reference_path: &Path,
    ) -> bool {
        let text = &reference.data().reference_text;
        match &self {
            Referenceable::Tag(_, _) => {
                matches!(reference, Tag(_))
                    && self.get_refname(root_dir).is_some_and(|refname| {
                        let refname_split = refname.split('/').collect_vec();
                        let text_split = text.split('/').collect_vec();

                        text_split.get(0..refname_split.len()) == Some(&refname_split)
                    })
            }
            Referenceable::Footnote(path, _footnote) => match reference {
                Footnote(..) => {
                    self.get_refname(root_dir).as_deref() == Some(text)
                        && path.as_path() == reference_path
                }
                MDFileLink(..) => false,
                Tag(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                LinkRef(_) => false,
                MystRole(..) => false,
                ImageLink(_) => false,
                Substitution(_) => false, // Substitutions don't reference footnotes
            },
            Referenceable::File(..) | Referenceable::UnresolvedFile(..) => match reference {
                MDFileLink(ReferenceData {
                    reference_text: file_ref_text,
                    ..
                })
                | MDHeadingLink(.., file_ref_text, _)
                | MDIndexedBlockLink(.., file_ref_text, _) => {
                    matches_path_or_file(file_ref_text, self.get_refname(root_dir))
                }
                // {doc}`path` role references files
                MystRole(_, MystRoleKind::Doc, target) => {
                    matches_path_or_file(target, self.get_refname(root_dir))
                }
                Tag(_) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
                MystRole(..) => false, // Other role types don't reference files
                ImageLink(_) => false, // Images reference files on disk, not markdown referenceables
                Substitution(_) => false, // Substitutions don't reference files
            },

            _ => reference.references(root_dir, reference_path, self),
        }
    }

    pub fn get_path(&self) -> &Path {
        match self {
            Referenceable::File(path, _) => path,
            Referenceable::Heading(path, _) => path,
            Referenceable::IndexedBlock(path, _) => path,
            Referenceable::Tag(path, _) => path,
            Referenceable::Footnote(path, _) => path,
            Referenceable::UnresolvedIndexedBlock(path, ..) => path,
            Referenceable::UnresolvedFile(path, ..) => path,
            Referenceable::UnresolvedHeading(path, ..) => path,
            Referenceable::LinkRefDef(path, ..) => path,
            Referenceable::MystAnchor(path, ..) => path,
            Referenceable::GlossaryTerm(path, ..) => path,
            Referenceable::MathLabel(path, ..) => path,
            Referenceable::SubstitutionDef(path, ..) => path,
        }
    }

    pub fn get_range(&self) -> Option<MyRange> {
        match self {
            Referenceable::File(_, _) => None,
            Referenceable::Heading(_, heading) => Some(heading.range),
            Referenceable::IndexedBlock(_, indexed_block) => Some(indexed_block.range),
            Referenceable::Tag(_, tag) => Some(tag.range),
            Referenceable::Footnote(_, footnote) => Some(footnote.range),
            Referenceable::LinkRefDef(_, refdef) => Some(refdef.range),
            Referenceable::UnresolvedFile(..)
            | Referenceable::UnresolvedHeading(..)
            | Referenceable::UnresolvedIndexedBlock(..) => None,
            Referenceable::MystAnchor(_, symbol) => Some(symbol.range),
            Referenceable::GlossaryTerm(_, term) => Some(term.range),
            Referenceable::MathLabel(_, symbol) => Some(symbol.range),
            // SubstitutionDef is defined in frontmatter - no specific range
            Referenceable::SubstitutionDef(_, _) => None,
        }
    }

    pub fn is_unresolved(&self) -> bool {
        matches!(
            self,
            Referenceable::UnresolvedHeading(..)
                | Referenceable::UnresolvedFile(..)
                | Referenceable::UnresolvedIndexedBlock(..)
        )
    }

    /// Returns a static string identifying the referenceable type.
    ///
    /// Useful for diagnostics, logging, and debugging without needing
    /// to match on all variants.
    #[allow(dead_code)] // API for future diagnostics/debugging use
    pub fn referenceable_type_name(&self) -> &'static str {
        match self {
            Referenceable::File(..) => "file",
            Referenceable::Heading(..) => "heading",
            Referenceable::IndexedBlock(..) => "indexed_block",
            Referenceable::Tag(..) => "tag",
            Referenceable::Footnote(..) => "footnote",
            Referenceable::UnresolvedFile(..) => "unresolved_file",
            Referenceable::UnresolvedHeading(..) => "unresolved_heading",
            Referenceable::UnresolvedIndexedBlock(..) => "unresolved_indexed_block",
            Referenceable::LinkRefDef(..) => "link_ref_def",
            Referenceable::MystAnchor(..) => "myst_anchor",
            Referenceable::GlossaryTerm(..) => "glossary_term",
            Referenceable::MathLabel(..) => "math_label",
            Referenceable::SubstitutionDef(..) => "substitution_def",
        }
    }

    /// Returns whether this referenceable type can be renamed.
    ///
    /// Rename is supported for:
    /// - File: Renames the file on disk
    /// - Heading: Changes the heading text
    /// - Tag: Updates all tag occurrences
    /// - MystAnchor: Changes the anchor name
    ///
    /// Other types (footnotes, indexed blocks, etc.) do not support rename.
    #[allow(dead_code)] // Documents rename-capable types for future UI hints
    pub fn is_renameable(&self) -> bool {
        matches!(
            self,
            Referenceable::File(..)
                | Referenceable::Heading(..)
                | Referenceable::Tag(..)
                | Referenceable::MystAnchor(..)
        )
    }

    /// Generate the definition-side edit for a rename operation.
    ///
    /// This method consolidates the logic for generating the edit that modifies
    /// the definition itself (heading text, file path, anchor name, etc.) when
    /// renaming a referenceable.
    ///
    /// # Arguments
    /// * `new_name` - The new name for the referenceable
    ///
    /// # Returns
    /// * `None` - This referenceable type doesn't support rename
    /// * `Some((None, ref_name))` - Rename supported but no definition edit needed (e.g., Tag)
    /// * `Some((Some(op), ref_name))` - Definition edit and the new reference name for updating refs
    ///
    /// The returned `ref_name` is used by `Reference::get_rename_text()` to update all
    /// references pointing to this referenceable.
    pub fn get_definition_rename_edit(
        &self,
        new_name: &str,
    ) -> Option<(Option<DocumentChangeOperation>, String)> {
        match self {
            Referenceable::Heading(path, heading) => {
                let new_text = format!("{} {}", "#".repeat(heading.level.0), new_name);

                let change_op = DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri: Url::from_file_path(path).ok()?,
                        version: None,
                    },
                    edits: vec![OneOf::Left(TextEdit {
                        range: *heading.range,
                        new_text,
                    })],
                });

                // Format: {filename}#{new heading name}
                let ref_name = format!("{}#{}", path.file_stem()?.to_string_lossy(), new_name);

                Some((Some(change_op), ref_name))
            }
            Referenceable::File(path, _file) => {
                let new_path = path.with_file_name(new_name).with_extension("md");

                let change_op = DocumentChangeOperation::Op(ResourceOp::Rename(RenameFile {
                    old_uri: Url::from_file_path(path).ok()?,
                    new_uri: Url::from_file_path(new_path).ok()?,
                    options: None,
                    annotation_id: None,
                }));

                Some((Some(change_op), new_name.to_string()))
            }
            Referenceable::Tag(_path, _tag) => {
                // Tags don't have a definition to edit, but they ARE renameable
                // (all tag references get updated)
                Some((None, new_name.to_string()))
            }
            Referenceable::MystAnchor(anchor_path, symbol) => {
                // Rename MyST anchor: (old-name)= -> (new-name)=
                let new_text = format!("({})=", new_name);

                let change_op = DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri: Url::from_file_path(anchor_path).ok()?,
                        version: None,
                    },
                    edits: vec![OneOf::Left(TextEdit {
                        range: *symbol.range,
                        new_text,
                    })],
                });

                Some((Some(change_op), new_name.to_string()))
            }
            // Other referenceable types don't support rename
            _ => None,
        }
    }
}

fn matches_path_or_file(file_ref_text: &str, refname: Option<Refname>) -> bool {
    (|| {
        let refname = refname?;
        let refname_path = refname.path.clone()?; // this function should not be used for tags, ... only for heading, files, indexed blocks

        if file_ref_text.contains('/') {
            let file_ref_text = file_ref_text.replace(r"%20", " ");
            let file_ref_text = file_ref_text.replace(r"\ ", " ");

            let chars: Vec<char> = file_ref_text.chars().collect();
            match chars.as_slice() {
                &['.', '/', ref path @ ..] | &['/', ref path @ ..] => {
                    Some(String::from_iter(path) == refname_path)
                }
                path => Some(String::from_iter(path) == refname_path),
            }
        } else {
            let last_segment = refname.link_file_key()?;

            Some(file_ref_text.to_lowercase() == last_segment.to_lowercase())
        }
    })()
    .is_some_and(|b| b)
}

/// Trait for checking if a file exists in the vault.
///
/// This abstraction allows both `HashMap<PathBuf, MDFile>` (used during construction)
/// and `Vault` (used during updates) to be used with the same resolution functions.
trait FileExistsChecker {
    fn file_exists(&self, path: &Path) -> bool;
    fn iter_paths(&self) -> Box<dyn Iterator<Item = &PathBuf> + '_>;
}

impl FileExistsChecker for HashMap<PathBuf, MDFile> {
    fn file_exists(&self, path: &Path) -> bool {
        self.contains_key(path)
    }

    fn iter_paths(&self) -> Box<dyn Iterator<Item = &PathBuf> + '_> {
        Box::new(self.keys())
    }
}

impl FileExistsChecker for Vault {
    fn file_exists(&self, path: &Path) -> bool {
        self.node_index.contains_key(path)
    }

    fn iter_paths(&self) -> Box<dyn Iterator<Item = &PathBuf> + '_> {
        Box::new(self.node_index.keys())
    }
}

/// Resolve a reference to its target file path, if it exists in the vault.
///
/// This function is used during graph construction to determine which files
/// a reference points to, enabling edge creation between document nodes.
///
/// # Arguments
///
/// * `reference` - The reference to resolve
/// * `source_path` - Path of the file containing the reference
/// * `root_dir` - Root directory of the vault
/// * `checker` - Anything implementing FileExistsChecker (HashMap or Vault)
///
/// # Returns
///
/// `Some(PathBuf)` if the reference resolves to an existing file in the vault,
/// `None` otherwise (e.g., for tags, footnotes, or unresolved file links).
fn resolve_reference_target(
    reference: &Reference,
    source_path: &Path,
    root_dir: &Path,
    checker: &impl FileExistsChecker,
) -> Option<PathBuf> {
    match reference {
        // File links: [text](path.md)
        Reference::MDFileLink(data) => {
            resolve_file_link(&data.reference_text, source_path, root_dir, checker)
        }
        // Heading links: [text](path.md#heading)
        Reference::MDHeadingLink(_data, file_ref, _heading) => {
            resolve_file_link(file_ref, source_path, root_dir, checker)
        }
        // Indexed block links: [text](path.md#^block-id)
        Reference::MDIndexedBlockLink(_data, file_ref, _block_id) => {
            resolve_file_link(file_ref, source_path, root_dir, checker)
        }
        // MyST {doc}`path` role
        Reference::MystRole(_data, MystRoleKind::Doc, target) => {
            resolve_file_link(target, source_path, root_dir, checker)
        }
        // Tags, footnotes, link refs, other roles don't resolve to files
        Reference::Tag(_)
        | Reference::Footnote(_)
        | Reference::LinkRef(_)
        | Reference::MystRole(..)
        | Reference::ImageLink(_)
        | Reference::Substitution(_) => None,
    }
}

/// Resolve a file reference text to an absolute path in the vault.
///
/// Handles various link formats:
/// - Relative paths: `target.md`, `../sibling/file.md`
/// - Absolute paths from root: `/path/to/file.md`
/// - Filename only: `file` (searches for `file.md`)
fn resolve_file_link(
    file_ref: &str,
    source_path: &Path,
    root_dir: &Path,
    checker: &impl FileExistsChecker,
) -> Option<PathBuf> {
    // Handle URL-encoded spaces
    let file_ref = file_ref.replace("%20", " ");
    let file_ref = file_ref.replace(r"\ ", " ");

    // Strip .md extension if present for matching
    let file_ref = file_ref.strip_suffix(".md").unwrap_or(&file_ref);

    // Try different resolution strategies

    // Strategy 1: Relative path from source file's directory
    if let Some(source_dir) = source_path.parent() {
        let relative_path = source_dir.join(format!("{}.md", file_ref));
        if checker.file_exists(&relative_path) {
            return Some(relative_path);
        }
    }

    // Strategy 2: Absolute path from vault root (for paths starting with /)
    if let Some(stripped) = file_ref.strip_prefix('/') {
        let absolute_path = root_dir.join(stripped).with_extension("md");
        if checker.file_exists(&absolute_path) {
            return Some(absolute_path);
        }
    }

    // Strategy 3: Path from vault root (for paths like dir/file)
    let from_root = root_dir.join(file_ref).with_extension("md");
    if checker.file_exists(&from_root) {
        return Some(from_root);
    }

    // Strategy 4: Search by filename only (case-insensitive)
    let file_ref_lower = file_ref.to_lowercase();
    for path in checker.iter_paths() {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem.to_lowercase() == file_ref_lower {
                return Some(path.clone());
            }
        }
    }

    None
}

/// Resolve a toctree/include entry to an absolute file path in the vault.
///
/// Toctree entries use the same resolution rules as file links:
/// - Relative paths from the source file's directory
/// - Absolute paths from vault root (starting with /)
/// - Paths from vault root (for paths like dir/file)
/// - Filename-only search (case-insensitive)
fn resolve_toctree_entry(
    entry: &str,
    source_path: &Path,
    root_dir: &Path,
    checker: &impl FileExistsChecker,
) -> Option<PathBuf> {
    resolve_file_link(entry, source_path, root_dir, checker)
}
