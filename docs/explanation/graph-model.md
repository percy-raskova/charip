---
title: Graph Model
---

# The Graph Model

charip-lsp uses a graph-based architecture to model documentation projects. This document explains why and how.

## Why a Graph?

### The Problem with Linear Storage

A naive approach stores files in a HashMap:

```rust
// Simple but slow
struct Vault {
    files: HashMap<PathBuf, ParsedFile>,
}
```

To find "what links to this file?", you must scan every file:

```rust
fn find_backlinks(&self, target: &Path) -> Vec<Location> {
    self.files.values()
        .flat_map(|f| &f.references)
        .filter(|r| r.resolves_to(target))
        .collect()  // O(n) where n = total references
}
```

This becomes slow with large documentation projects.

### The Graph Solution

With a graph, relationships are explicit edges:

```rust
struct Vault {
    graph: DiGraph<DocumentNode, EdgeKind>,
    node_index: HashMap<PathBuf, NodeIndex>,
}
```

Finding backlinks is now a graph traversal:

```rust
fn find_backlinks(&self, target: &Path) -> Vec<Location> {
    let node = self.node_index[target];
    self.graph
        .neighbors_directed(node, Incoming)
        .collect()  // O(k) where k = incoming edges
}
```

## Graph Structure

### Nodes

Each document is a node containing parsed data:

```rust
pub struct DocumentNode {
    pub path: PathBuf,
    pub references: Vec<Reference>,
    pub headings: Vec<MDHeading>,
    pub myst_symbols: Vec<MystSymbol>,
    // ...
}
```

### Edges

Relationships between documents are edges:

```rust
pub enum EdgeKind {
    /// Standard link or reference
    Reference,
    /// Toctree entry (structural relationship)
    Structure { caption: Option<String> },
    /// Include directive (content embedding)
    Transclusion { range: Option<Range> },
}
```

### The Type

charip-lsp uses petgraph's directed graph:

```rust
pub type VaultGraph = DiGraph<DocumentNode, EdgeKind>;
```

## Operations

### Node Lookup

Files are found via the index HashMap:

```rust
pub fn get_document(&self, path: &Path) -> Option<&DocumentNode> {
    self.node_index.get(path).map(|&idx| &self.graph[idx])
}
```

Time complexity: O(1)

### Forward References

Find what a document links to:

```rust
pub fn outgoing_references(&self, path: &Path) -> Vec<&Reference> {
    let node = self.get_document(path)?;
    node.references.iter().collect()
}
```

### Backlinks

Find what links to a document:

```rust
pub fn incoming_references(&self, path: &Path) -> Vec<Location> {
    let idx = self.node_index.get(path)?;
    self.graph
        .neighbors_directed(*idx, Direction::Incoming)
        .flat_map(|source_idx| {
            // Find references from source that point here
        })
        .collect()
}
```

## Structural Analysis

The graph enables analyses not possible with flat storage.

### Cycle Detection

Circular includes cause infinite loops in Sphinx. charip-lsp detects them:

```rust
pub fn detect_include_cycles(&self) -> Vec<Vec<PathBuf>> {
    // Filter to include edges only
    let include_graph = self.graph.filter_map(
        |_, node| Some(node),
        |_, edge| match edge {
            EdgeKind::Transclusion { .. } => Some(()),
            _ => None,
        }
    );

    // Tarjan's SCC algorithm finds cycles
    tarjan_scc(&include_graph)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .collect()
}
```

### Orphan Detection

Documents not in any toctree are orphans:

```rust
pub fn find_orphans(&self, root: &Path) -> Vec<PathBuf> {
    // BFS from root following Structure edges
    let reachable = self.bfs_reachable(root, EdgeKind::Structure);

    // Nodes not in reachable set are orphans
    self.graph.node_indices()
        .filter(|idx| !reachable.contains(idx))
        .map(|idx| self.graph[idx].path.clone())
        .collect()
}
```

### Transitive Dependencies

Find all documents that a file depends on:

```rust
pub fn transitive_dependencies(&self, path: &Path) -> HashSet<PathBuf> {
    let idx = self.node_index.get(path)?;

    // DFS following all outgoing edges
    let mut deps = HashSet::new();
    let mut dfs = Dfs::new(&self.graph, *idx);

    while let Some(node_idx) = dfs.next(&self.graph) {
        deps.insert(self.graph[node_idx].path.clone());
    }

    deps
}
```

## Building the Graph

### Initial Construction

On startup, charip-lsp:

1. Walks the directory tree
2. Parses each `.md` file in parallel (rayon)
3. Creates nodes for each file
4. Resolves references to create edges

### Incremental Updates

On file change:

1. Re-parse the changed file
2. Remove old edges from this node
3. Add new edges based on updated references
4. Node content is updated in place

## Performance Characteristics

| Operation | Complexity |
|-----------|------------|
| Get document by path | O(1) |
| Forward references | O(1) |
| Backlinks | O(k) edges |
| Cycle detection | O(V + E) |
| Orphan detection | O(V + E) |
| Transitive deps | O(V + E) |

Where V = documents, E = references.

## Trade-offs

### Memory Usage

The graph stores more data than a simple file list:
- Node indices in HashMap
- Edge metadata
- Reference data duplicated on edges

For most documentation projects, this is negligible.

### Update Complexity

Maintaining graph consistency requires careful edge management. When a file changes, its outgoing edges must be recalculated.

### Benefits

The graph model enables features that would be impractical otherwise:
- Instant backlinks
- Structural validation
- Impact analysis
- Visualization (future)
