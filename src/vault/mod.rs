mod ast_refs;
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

        Ok(Vault {
            ropes: ropes.into(),
            md_files: md_files.into(),
            root_dir: root_dir.into(),
        })
    }

    pub fn update_vault(context: &Settings, old: &mut Vault, new_file: (&PathBuf, &str)) {
        let new_md_file = MDFile::new(context, new_file.1, new_file.0.clone());
        let new = old.md_files.get_mut(new_file.0);
        match new {
            Some(file) => {
                *file = new_md_file;
            }
            None => {
                old.md_files.insert(new_file.0.into(), new_md_file);
            }
        };

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
        self.md_files.hash(state)
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

#[derive(Debug, PartialEq, Eq, Clone)]
/// The in-memory representation of the Markdown vault files. This data is exposed through an interface of methods to select the vault's data.
/// These methods do not do any interpretation or analysis of the data. That is up to the consumer of this struct. The methods are analogous to selecting on a database.
pub struct Vault {
    pub md_files: MyHashMap<MDFile>,
    pub ropes: MyHashMap<Rope>,
    root_dir: PathBuf,
}

/// Methods using vaults data
impl Vault {
    /// Generic helper for selecting Vec fields from MDFile with optional path filtering.
    ///
    /// If `path` is Some, returns items from that file only.
    /// If `path` is None, returns items from all files in the vault.
    fn select_field<'a, T>(
        &'a self,
        path: Option<&'a Path>,
        extractor: impl Fn(&'a MDFile) -> &'a Vec<T>,
    ) -> Vec<(&'a Path, &'a T)> {
        match path {
            Some(path) => self
                .md_files
                .get(path)
                .map(&extractor)
                .map(|vec| vec.iter().map(|item| (path, item)).collect())
                .unwrap_or_default(),
            None => self
                .md_files
                .iter()
                .flat_map(|(path, md)| extractor(md).iter().map(|item| (path.as_path(), item)))
                .collect(),
        }
    }

    /// Select all references ([[link]] or #tag) in a file if path is Some, else all in vault.
    pub fn select_references<'a>(
        &'a self,
        path: Option<&'a Path>,
    ) -> Vec<(&'a Path, &'a Reference)> {
        self.select_field(path, |md| &md.references)
    }

    /// Select all MyST symbols in a file if path is Some, else all in vault.
    #[allow(dead_code)] // Public API for consumers; not used internally yet
    pub fn select_myst_symbols<'a>(
        &'a self,
        path: Option<&'a Path>,
    ) -> Vec<(&'a Path, &'a MystSymbol)> {
        self.select_field(path, |md| &md.myst_symbols)
    }

    /// Select MyST directives (```{note}, ```{warning}, etc.) in a file or vault.
    #[allow(dead_code)] // Public API for consumers; not used internally yet
    pub fn select_myst_directives<'a>(
        &'a self,
        path: Option<&'a Path>,
    ) -> Vec<(&'a Path, &'a MystSymbol)> {
        self.select_myst_symbols(path)
            .into_iter()
            .filter(|(_, s)| s.kind == MystSymbolKind::Directive)
            .collect()
    }

    /// Select MyST anchors ((target-name)=) in a file or vault.
    #[allow(dead_code)] // Public API for consumers; not used internally yet
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
                .md_files
                .iter()
                .find(|(iterpath, _)| *iterpath == path)
                .map(|(pathbuf, mdfile)| Referenceable::File(pathbuf, mdfile)),
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
                    iter::once(self.md_files.get(path).map(|md| md.get_referenceables()))
                        .flatten()
                        .flatten()
                        .collect_vec();

                resolved_referenceables

                // TODO: Add unresolved referenceables
            }
            None => {
                let resolved_referenceables = self
                    .md_files
                    .values()
                    .par_bridge()
                    .into_par_iter()
                    .flat_map(|file| file.get_referenceables())
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
        let md_file = self.md_files.get(path)?;
        let headings = &md_file.headings;
        Some(headings)
    }

    pub fn root_dir(&self) -> &PathBuf {
        &self.root_dir
    }

    pub fn select_references_for_referenceable(
        &self,
        referenceable: &Referenceable,
    ) -> Option<Vec<(&Path, &Reference)>> {
        let references = self.select_references(None);

        Some(
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
                .collect(),
        )
    }

    pub fn select_referenceables_for_reference(
        &self,
        reference: &Reference,
        reference_path: &Path,
    ) -> Vec<Referenceable<'_>> {
        let referenceables = self.select_referenceable_nodes(None);

        referenceables
            .into_iter()
            .filter(|i| reference.references(self.root_dir(), reference_path, i))
            .collect()
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

    pub fn file_name(&self) -> Option<&str> {
        self.path.file_stem()?.to_str()
    }
}

impl MDFile {
    fn get_referenceables(&self) -> Vec<Referenceable<'_>> {
        let MDFile {
            references: _,
            headings,
            indexed_blocks,
            tags,
            footnotes,
            path: _,
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
            .collect()
    }
}

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
    File(&'a PathBuf, &'a MDFile),
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
                let ref_name = format!(
                    "{}#{}",
                    path.file_stem()?.to_string_lossy(),
                    new_name
                );

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
