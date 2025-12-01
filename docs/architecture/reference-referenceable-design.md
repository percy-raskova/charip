# Reference and Referenceable: Design Rationale

## Overview

The `Reference` and `Referenceable` types in `src/vault/mod.rs` have a bidirectional relationship. This document explains why this design exists and why it should be preserved.

## Type Definitions

**Reference** (`src/vault/mod.rs`, lines 694-702):
```rust
pub enum Reference {
    Tag(ReferenceData),
    MDFileLink(ReferenceData),
    MDHeadingLink(ReferenceData, File, Specialref),
    MDIndexedBlockLink(ReferenceData, File, Specialref),
    Footnote(ReferenceData),
    LinkRef(ReferenceData),
}
```

**Referenceable** (`src/vault/mod.rs`, lines 992-1004):
```rust
pub enum Referenceable<'a> {
    File(&'a PathBuf, &'a MDFile),
    Heading(&'a PathBuf, &'a MDHeading),
    IndexedBlock(&'a PathBuf, &'a MDIndexedBlock),
    Tag(&'a PathBuf, &'a MDTag),
    Footnote(&'a PathBuf, &'a MDFootnote),
    UnresovledFile(PathBuf, &'a String),
    UnresolvedHeading(PathBuf, &'a String, &'a String),
    UnresovledIndexedBlock(PathBuf, &'a String, &'a String),
    LinkRefDef(&'a PathBuf, &'a MDLinkReferenceDefinition),
}
```

## The Bidirectional Relationship

Two methods create the coupling:

1. `Reference::references(&Referenceable)` - lines 763-847
2. `Referenceable::matches_reference(&Reference)` - lines 1075-1119

### Why Both Methods Exist

`matches_reference()` delegates to `references()` for most cases (line 1117):
```rust
_ => reference.references(self, root_dir, path),
```

Special handling exists only for:

| Variant | Reason | Code Location |
|---------|--------|---------------|
| `Referenceable::Tag` | Nested tag matching required | lines 1081-1093 |
| `Referenceable::Footnote` | Path-scoped matching required | lines 1095-1105 |
| `Referenceable::File` | Multiple Reference types can target a File | lines 1107-1115 |

## Historical Origin

Git commit `81bea62` (2024-02-16) introduced both types and both methods together. The bidirectional design was intentional from the initial implementation.

## LSP Operations Enabled

| Operation | Method Used | Direction |
|-----------|-------------|-----------|
| Go-to-definition | `Reference::references()` | Reference → Referenceable |
| Find-references | `Referenceable::matches_reference()` | Referenceable → Reference |

## External Consumers

Files that use `Reference`:
- `src/rename.rs`
- `src/diagnostics.rs`
- `src/ui.rs`
- `src/codeactions.rs`
- `src/completion/link_completer.rs`
- `src/vault/ast_refs.rs`

Files that use `Referenceable`:
- `src/rename.rs`
- `src/references.rs`
- `src/gotodef.rs`
- `src/codelens.rs`
- `src/diagnostics.rs`
- `src/completion/*.rs`
- `src/ui.rs`

## Why This Design Should Be Preserved

1. **Domain accuracy**: The types model two sides of a linking relationship. Their coupling reflects the problem domain.

2. **Functional correctness**: All 77 tests pass. The matching logic works correctly.

3. **Delegation pattern**: `matches_reference()` delegates to `references()` for most cases, avoiding duplication.

4. **Rust compatibility**: Bidirectional method references between types in the same module are valid Rust. This is not a circular dependency in the compilation sense.

## Alternatives Considered and Rejected

| Alternative | Rejection Reason |
|-------------|------------------|
| Trait-based decoupling | Over-engineered; loses exhaustive pattern matching |
| Mediator pattern | Adds indirection without functional benefit |
| Inline to consumers | Violates DRY; creates maintenance burden |

## Conclusion

The bidirectional relationship between `Reference` and `Referenceable` is a deliberate design choice that accurately models the domain. Refactoring to decouple them would add complexity without improving functionality.
