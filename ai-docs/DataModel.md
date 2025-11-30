# Data Model

The core of Markdown-Oxide is the `Vault` module, which models the "Knowledge Graph" of the user's notes.

## Core Structures

### 1. `Vault`
The container for all data.
```rust
pub struct Vault {
    pub md_files: MyHashMap<MDFile>, // Parsed structure
    pub ropes: MyHashMap<Rope>,      // Raw text content
    root_dir: PathBuf,
}
```

### 2. `MDFile`
Represents a single parsed Markdown file.
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
}
```
*   **Parsing**: Occurs in `MDFile::new()`. Uses various Regex patterns to extract these elements.

### 3. `Reference` (The "Edge")
Represents an *outgoing* link from a file.
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
*   `ReferenceData` holds the raw text, display text (alias), and the range (start/end position) in the file.

### 4. `Referenceable` (The "Node")
Represents something that *can be linked to*. This serves as a unified interface for different target types.
```rust
pub enum Referenceable<'a> {
    File(&'a PathBuf, &'a MDFile),
    Heading(&'a PathBuf, &'a MDHeading),
    IndexedBlock(&'a PathBuf, &'a MDIndexedBlock),
    Tag(&'a PathBuf, &'a MDTag),
    Footnote(&'a PathBuf, &'a MDFootnote),
    // Unresolved variants for links pointing to non-existent targets
    UnresovledFile(PathBuf, &'a String),
    UnresolvedHeading(PathBuf, &'a String, &'a String),
    UnresovledIndexedBlock(PathBuf, &'a String, &'a String),
    LinkRefDef(&'a PathBuf, &'a MDLinkReferenceDefinition),
}
```

### 5. `Rangeable` Trait
A trait implemented by most structures (`MDHeading`, `Reference`, etc.) to standardize access to their position (`MyRange`) in the document.

## Relationships & Lookup

*   **References vs. Referenceables**: The system distinguishes between the *source* of a link (`Reference`) and the *target* (`Referenceable`).
*   **Resolution**:
    *   `Vault::select_references(path)`: Gets all outgoing links in a file.
    *   `Vault::select_referenceable_nodes(path)`: Gets all valid targets in a file.
    *   `Vault::select_references_for_referenceable(target)`: The "Backlinks" query. It iterates through *all* references in the vault and checks if they match the given `target` using `matches_reference()`.

## Parsing Strategy
*   **Regex**: Currently, parsing is primarily Regex-based (lazy static compilation).
*   **Ropey**: Used to convert byte offsets from Regex matches into Line/Character coordinates required by LSP.
*   **Parsing Logic**: Located in `src/vault/mod.rs` (impls for `MDHeading::new`, `Reference::new`, etc.) and `src/vault/parsing.rs` (`MDCodeBlock`).
