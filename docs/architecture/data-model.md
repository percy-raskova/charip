---
title: Data Model
---

# Data Model

The core of {{project}} is the `Vault` module, which models the "Knowledge Graph" of the user's notes.

## Core Structures

### Vault

The container for all data:

```rust
pub struct Vault {
    pub md_files: MyHashMap<MDFile>, // Parsed structure
    pub ropes: MyHashMap<Rope>,      // Raw text content
    root_dir: PathBuf,
}
```

### MDFile

Represents a single parsed Markdown file:

```rust
pub struct MDFile {
    pub references: Vec<Reference>,
    pub headings: Vec<MDHeading>,
    pub indexed_blocks: Vec<MDIndexedBlock>,
    pub tags: Vec<MDTag>,
    pub footnotes: Vec<MDFootnote>,
    pub path: PathBuf,
    pub link_reference_definitions: Vec<MDLinkReferenceDefinition>,
    pub metadata: Option<MDMetadata>, // YAML Frontmatter
    pub codeblocks: Vec<MDCodeBlock>,
    pub myst_symbols: Vec<MystSymbol>, // MyST directives & anchors
}
```

```{note}
Parsing occurs in `MDFile::new()`. Uses various Regex patterns to extract these elements, plus `myst_parser::parse()` for MyST symbols.
```

### Reference (The "Edge")

Represents an *outgoing* link from a file:

```rust
pub enum Reference {
    Tag(ReferenceData),
    WikiFileLink(ReferenceData),
    WikiHeadingLink(ReferenceData, File, Specialref),
    WikiIndexedBlockLink(ReferenceData, File, Specialref),
    MDFileLink(ReferenceData),
    MDHeadingLink(ReferenceData, File, Specialref),
    MDIndexedBlockLink(ReferenceData, File, Specialref),
    Footnote(ReferenceData),
    LinkRef(ReferenceData),
}
```

`ReferenceData`
: Holds the raw text, display text (alias), and the range (start/end position) in the file.

### Referenceable (The "Node")

Represents something that *can be linked to*—a unified interface for different target types:

```rust
pub enum Referenceable<'a> {
    File(&'a PathBuf, &'a MDFile),
    Heading(&'a PathBuf, &'a MDHeading),
    IndexedBlock(&'a PathBuf, &'a MDIndexedBlock),
    Tag(&'a PathBuf, &'a MDTag),
    Footnote(&'a PathBuf, &'a MDFootnote),
    UnresolvedFile(PathBuf, &'a String),
    UnresolvedHeading(PathBuf, &'a String, &'a String),
    UnresolvedIndexedBlock(PathBuf, &'a String, &'a String),
    LinkRefDef(&'a PathBuf, &'a MDLinkReferenceDefinition),
}
```

### MystSymbol

Represents MyST-specific constructs:

```rust
pub enum MystSymbolKind {
    Directive,   // ```{note}, ```{warning}, etc.
    Anchor,      // (target-name)=
    Reference,   // {ref}`target` (future)
}

pub struct MystSymbol {
    pub kind: MystSymbolKind,
    pub name: String,
    pub line: usize,
}
```

### Rangeable Trait

A trait implemented by most structures (`MDHeading`, `Reference`, etc.) to standardize access to their position (`MyRange`) in the document.

## Relationships & Lookup

```{list-table} Query Methods
:header-rows: 1
:widths: 40 60

* - Method
  - Purpose
* - `select_references(path)`
  - Gets all outgoing links in a file
* - `select_referenceable_nodes(path)`
  - Gets all valid targets in a file
* - `select_references_for_referenceable(target)`
  - The "Backlinks" query—iterates through all references to find matches
* - `select_myst_symbols(path)`
  - Gets all MyST symbols (directives, anchors) in a file or vault
```

## Parsing Strategy

Current Approach
: Primarily Regex-based (lazy static compilation) with `myst_parser` for MyST constructs.

Position Mapping
: `ropey` converts byte offsets from Regex matches into Line/Character coordinates required by LSP.

Location
: Parsing logic is in `src/vault/mod.rs` and `src/vault/parsing.rs`.

```{seealso}
{doc}`deep-dive` for discussion of the planned transition from Regex to AST-based parsing.
```
