mod helpers;
mod metadata;
mod parsing;
mod types;

#[cfg(test)]
mod tests;

pub use helpers::{get_obsidian_ref_path, Refname};
pub use types::{
    HeadingLevel, MDFootnote, MDHeading, MDIndexedBlock, MDLinkReferenceDefinition, MDTag, MyRange,
    Rangeable,
};

use std::{
    char,
    collections::{HashMap, HashSet},
    hash::Hash,
    iter,
    ops::{Deref, DerefMut, Not},
    path::{Path, PathBuf},
    time::SystemTime,
};

use itertools::Itertools;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::{Captures, Match, Regex};
use ropey::Rope;
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{Location, Position, SymbolInformation, SymbolKind, Url};
use walkdir::WalkDir;

impl Vault {
    pub fn construct_vault(context: &Settings, root_dir: &Path) -> Result<Vault, std::io::Error> {
        let md_file_paths = WalkDir::new(root_dir)
            .into_iter()
            .filter_entry(|e| {
                !e.file_name()
                    .to_str()
                    .map(|s| s.starts_with('.') || s == "logseq") // TODO: This is a temporary fix; a hidden config is better
                    .unwrap_or(false)
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
        _ => match referenceable.get_range() {
            None => None,
            Some(a_my_range) => Some(*a_my_range),
        },
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
/// The in memory representation of the obsidian vault files. This data is exposed through an interface of methods to select the vaults data.
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
                .map(|md| extractor(md))
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
    pub fn select_myst_symbols<'a>(
        &'a self,
        path: Option<&'a Path>,
    ) -> Vec<(&'a Path, &'a MystSymbol)> {
        self.select_field(path, |md| &md.myst_symbols)
    }

    /// Select MyST directives (```{note}, ```{warning}, etc.) in a file or vault.
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
                        !resolved_referenceables_refnames
                            .contains(&reference.data().reference_text)
                    })
                    .flat_map(|(_, reference)| match reference {
                        Reference::WikiFileLink(data) | Reference::MDFileLink(data) => {
                            let mut path = self.root_dir().clone();
                            path.push(&reference.data().reference_text);

                            Some(Referenceable::UnresovledFile(path, &data.reference_text))

                            // match data.reference_text.chars().collect_vec().as_slice() {

                            //     [..,'.','m','d'] =>
                            //     ['.', '/', rest @ ..]
                            //     | ['/', rest @ ..]
                            //     | rest if !rest.contains(&'.') => Some(Referenceable::UnresovledFile(path, &data.reference_text)),
                            //     _ => None
                            // }
                        }
                        Reference::WikiHeadingLink(_data, end_path, heading)
                        | Reference::MDHeadingLink(_data, end_path, heading) => {
                            let mut path = self.root_dir().clone();
                            path.push(end_path);

                            Some(Referenceable::UnresolvedHeading(path, end_path, heading))
                        }
                        Reference::WikiIndexedBlockLink(_data, end_path, index)
                        | Reference::MDIndexedBlockLink(_data, end_path, index) => {
                            let mut path = self.root_dir().clone();
                            path.push(end_path);

                            Some(Referenceable::UnresovledIndexedBlock(path, end_path, index))
                        }
                        Reference::Tag(..)
                        | Reference::Footnote(..)
                        | Reference::LinkRef(..) => None,
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
            Referenceable::UnresovledFile(_, _) => None,
            Referenceable::UnresolvedHeading(_, _, _) => None,
            Referenceable::UnresovledIndexedBlock(_, _, _) => None,
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
}

impl MDFile {
    fn new(context: &Settings, text: &str, path: PathBuf) -> MDFile {
        let myst_symbols = myst_parser::parse(text);
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
            myst_symbols: _,
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

#[derive(Debug, PartialEq, Eq, Clone)]
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

impl Deref for Reference {
    type Target = ReferenceData;
    fn deref(&self) -> &Self::Target {
        self.data()
    }
}

impl Default for Reference {
    fn default() -> Self {
        WikiFileLink(ReferenceData::default())
    }
}

use Reference::*;

use crate::config::Settings;
use crate::myst_parser::{self, MystSymbol, MystSymbolKind};

use self::{metadata::MDMetadata, parsing::MDCodeBlock};

impl Reference {
    pub fn data(&self) -> &ReferenceData {
        match &self {
            Tag(data, ..) => data,
            WikiFileLink(data, ..) => data,
            WikiHeadingLink(data, ..) => data,
            WikiIndexedBlockLink(data, ..) => data,
            Footnote(data) => data,
            MDFileLink(data, ..) => data,
            MDHeadingLink(data, ..) => data,
            MDIndexedBlockLink(data, ..) => data,
            LinkRef(data, ..) => data,
        }
    }

    pub fn matches_type(&self, other: &Reference) -> bool {
        match &other {
            Tag(..) => matches!(self, Tag(..)),
            WikiFileLink(..) => matches!(self, WikiFileLink(..)),
            WikiHeadingLink(..) => matches!(self, WikiHeadingLink(..)),
            WikiIndexedBlockLink(..) => matches!(self, WikiIndexedBlockLink(..)),
            Footnote(..) => matches!(self, Footnote(..)),
            MDFileLink(..) => matches!(self, MDFileLink(..)),
            MDHeadingLink(..) => matches!(self, MDHeadingLink(..)),
            MDIndexedBlockLink(..) => matches!(self, MDIndexedBlockLink(..)),
            LinkRef(..) => matches!(self, LinkRef(..)),
        }
    }

    pub fn new<'a>(text: &'a str, file_name: &'a str) -> impl Iterator<Item = Reference> + 'a {
        static WIKI_LINK_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\[\[(?<filepath>[^\[\]\|\.\#]+)?(\#(?<infileref>[^\[\]\.\|]+))?(?<ending>\.[^\# <>]+)?(\|(?<display>[^\[\]\.\|]+))?\]\]")

                .unwrap()
        }); // A [[link]] that does not have any [ or ] in it

        let wiki_links = WIKI_LINK_RE
            .captures_iter(text)
            .filter(|captures| {
                matches!(
                    captures.name("ending").map(|ending| ending.as_str()),
                    Some(".md") | None
                )
            })
            .flat_map(RegexTuple::new)
            .flat_map(|regextuple| {
                generic_link_constructor::<WikiReferenceConstructor>(text, file_name, regextuple)
            });

        static MD_LINK_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\[(?<display>[^\[\]\.]*?)\]\(<?(?<filepath>(\.?\/)?[^\[\]\|\.\#<>]+?)?(?<ending>\.[^\# <>]+?)?(\#(?<infileref>[^\[\]\.\|<>]+?))?>?\)")
                .expect("MD Link Not Constructing")
        }); // [display](relativePath)

        let md_links = MD_LINK_RE
            .captures_iter(text)
            .filter(|captures| {
                matches!(
                    captures.name("ending").map(|ending| ending.as_str()),
                    Some(".md") | None
                )
            })
            .flat_map(RegexTuple::new)
            .flat_map(|regextuple| {
                generic_link_constructor::<MDReferenceConstructor>(text, file_name, regextuple)
            });

        let tags = MDTag::new(text).map(|tag| {
            Tag(ReferenceData {
                display_text: None,
                range: tag.range,
                reference_text: format!("#{}", tag.tag_ref),
            })
        });

        static FOOTNOTE_LINK_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?<start>\[?)(?<full>\[(?<index>\^[^\[\] ]+)\])(?<end>:?)").unwrap()
        });
        let footnote_references = FOOTNOTE_LINK_RE
            .captures_iter(text)
            .filter(|capture| matches!(
                (capture.name("start"), capture.name("end")),
                (Some(start), Some(end)) if !start.as_str().starts_with('[') && !end.as_str().ends_with(':'))
            )
            .flat_map(
                |capture| match (capture.name("full"), capture.name("index")) {
                    (Some(full), Some(index)) => Some((full, index)),
                    _ => None,
                },
            )
            .map(|(outer, index)| {
                Footnote(ReferenceData {
                    reference_text: index.as_str().into(),
                    range: MyRange::from_range(&Rope::from_str(text), outer.range()),
                    display_text: None,
                })
            });

        let link_ref_references: Vec<Reference> = if MDLinkReferenceDefinition::new(text)
            .collect_vec()
            .is_empty()
            .not()
        {
            static LINK_REF_RE: Lazy<Regex> = Lazy::new(|| {
                Regex::new(r"([^\[]|^)(?<full>\[(?<index>[^\^][^\[\] ]+)\])([^\]\(\:]|$)").unwrap()
            });

            let link_ref_references: Vec<Reference> = LINK_REF_RE
                .captures_iter(text)
                .par_bridge()
                .flat_map(
                    |capture| match (capture.name("full"), capture.name("index")) {
                        (Some(full), Some(index)) => Some((full, index)),
                        _ => None,
                    },
                )
                .map(|(outer, index)| {
                    LinkRef(ReferenceData {
                        reference_text: index.as_str().into(),
                        range: MyRange::from_range(&Rope::from_str(text), outer.range()),
                        display_text: None,
                    })
                })
                .collect::<Vec<_>>();

            link_ref_references
        } else {
            vec![]
        };

        wiki_links
            .into_iter()
            .chain(md_links)
            .chain(tags)
            .chain(footnote_references)
            .chain(link_ref_references)
    }

    pub fn references(
        &self,
        root_dir: &Path,
        file_path: &Path,
        referenceable: &Referenceable,
    ) -> bool {
        let text = &self.data().reference_text;
        match referenceable {
            &Referenceable::Tag(_, _) => {
                match self {
                    Tag(..) => {
                        referenceable
                            .get_refname(root_dir)
                            .map(|thing| thing.to_string())
                            == Some(text.to_string())
                    }

                    WikiFileLink(_) => false,
                    WikiHeadingLink(_, _, _) => false,
                    WikiIndexedBlockLink(_, _, _) => false,
                    MDFileLink(_) => false,
                    MDHeadingLink(_, _, _) => false,
                    MDIndexedBlockLink(_, _, _) => false,
                    Footnote(_) => false,
                    LinkRef(_) => false, // (no I don't write all of these by hand; I use rust-analyzers code action; I do this because when I add new item to the Reference enum, I want workspace errors everywhere relevant)
                }
            }
            &Referenceable::Footnote(path, _footnote) => match self {
                Footnote(..) => {
                    referenceable.get_refname(root_dir).as_deref() == Some(text)
                        && path.as_path() == file_path
                }
                Tag(_) => false,
                WikiFileLink(_) => false,
                WikiHeadingLink(_, _, _) => false,
                WikiIndexedBlockLink(_, _, _) => false,
                MDFileLink(_) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                LinkRef(_) => false,
            },
            &Referenceable::File(..) | &Referenceable::UnresovledFile(..) => match self {
                MDFileLink(ReferenceData {
                    reference_text: file_ref_text,
                    ..
                })
                | WikiFileLink(ReferenceData {
                    reference_text: file_ref_text,
                    ..
                }) => matches_path_or_file(file_ref_text, referenceable.get_refname(root_dir)),
                Tag(_) => false,
                WikiHeadingLink(_, _, _) => false,
                WikiIndexedBlockLink(_, _, _) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
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
            | &Referenceable::UnresovledIndexedBlock(.., infile_ref) => match self {
                WikiHeadingLink(.., file_ref_text, link_infile_ref)
                | WikiIndexedBlockLink(.., file_ref_text, link_infile_ref)
                | MDHeadingLink(.., file_ref_text, link_infile_ref)
                | MDIndexedBlockLink(.., file_ref_text, link_infile_ref) => {
                    matches_path_or_file(file_ref_text, referenceable.get_refname(root_dir))
                        && link_infile_ref.to_lowercase() == infile_ref.to_lowercase()
                }
                Tag(_) => false,
                WikiFileLink(_) => false,
                MDFileLink(_) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
            },
            Referenceable::LinkRefDef(path, _link_ref) => match self {
                Tag(_) => false,
                WikiFileLink(_) => false,
                WikiHeadingLink(_, _, _) => false,
                WikiIndexedBlockLink(_, _, _) => false,
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
            },
        }
    }
}

struct RegexTuple<'a> {
    range: Match<'a>,
    file_path: Option<Match<'a>>,
    infile_ref: Option<Match<'a>>,
    display_text: Option<Match<'a>>,
}

impl RegexTuple<'_> {
    fn new(capture: Captures) -> Option<RegexTuple> {
        match (
            capture.get(0),
            capture.name("filepath"),
            capture.name("infileref"),
            capture.name("display"),
        ) {
            (Some(range), file_path, infile_ref, display_text) => Some(RegexTuple {
                range,
                file_path,
                infile_ref,
                display_text,
            }),
            _ => None,
        }
    }
}

trait ParseableReferenceConstructor {
    fn new_heading(data: ReferenceData, path: &str, heading: &str) -> Reference;
    fn new_file_link(data: ReferenceData) -> Reference;
    fn new_indexed_block_link(data: ReferenceData, path: &str, index: &str) -> Reference;
} // TODO: Turn this into a macro

struct WikiReferenceConstructor;
struct MDReferenceConstructor;

impl ParseableReferenceConstructor for WikiReferenceConstructor {
    fn new_heading(data: ReferenceData, path: &str, heading: &str) -> Reference {
        Reference::WikiHeadingLink(data, path.into(), heading.into())
    }
    fn new_file_link(data: ReferenceData) -> Reference {
        Reference::WikiFileLink(data)
    }
    fn new_indexed_block_link(data: ReferenceData, path: &str, index: &str) -> Reference {
        Reference::WikiIndexedBlockLink(data, path.into(), index.into())
    }
}

impl ParseableReferenceConstructor for MDReferenceConstructor {
    fn new_heading(data: ReferenceData, path: &str, heading: &str) -> Reference {
        Reference::MDHeadingLink(data, path.into(), heading.into())
    }
    fn new_file_link(data: ReferenceData) -> Reference {
        Reference::MDFileLink(data)
    }
    fn new_indexed_block_link(data: ReferenceData, path: &str, index: &str) -> Reference {
        Reference::MDIndexedBlockLink(data, path.into(), index.into())
    }
}

fn generic_link_constructor<T: ParseableReferenceConstructor>(
    text: &str,
    file_name: &str,
    RegexTuple {
        range,
        file_path,
        infile_ref,
        display_text,
    }: RegexTuple,
) -> Option<Reference> {
    if file_path.is_some_and(|path| {
        path.as_str().starts_with("http://")
            || path.as_str().starts_with("https://")
            || path.as_str().starts_with("data:")
    }) {
        return None;
    }

    let decoded_filepath = file_path
        .map(|file_path| {
            let file_path = file_path.as_str();
            urlencoding::decode(file_path).map_or_else(|_| file_path.to_string(), |d| d.to_string())
        })
        .unwrap_or_else(|| file_name.to_string());

    match (range, decoded_filepath.as_str(), infile_ref, display_text) {
        // Pure file reference as there is no infileref such as #... for headings or #^... for indexed blocks
        (full, filepath, None, display) => Some(T::new_file_link(ReferenceData {
            reference_text: filepath.into(),
            range: MyRange::from_range(&Rope::from_str(text), full.range()),
            display_text: display.map(|d| d.as_str().into()),
        })),
        (full, filepath, Some(infile), display) if infile.as_str().get(0..1) == Some("^") => {
            Some(T::new_indexed_block_link(
                ReferenceData {
                    reference_text: format!("{}#{}", filepath, infile.as_str()),
                    range: MyRange::from_range(&Rope::from_str(text), full.range()),
                    display_text: display.map(|d| d.as_str().into()),
                },
                filepath,
                &infile.as_str()[1..], // drop the ^ for the index
            ))
        }
        (full, filepath, Some(infile), display) => Some(T::new_heading(
            ReferenceData {
                reference_text: format!("{}#{}", filepath, infile.as_str()),
                range: MyRange::from_range(&Rope::from_str(text), full.range()),
                display_text: display.map(|d| d.as_str().into()),
            },
            filepath,
            infile.as_str(),
        )),
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
An Algebreic type for methods for all referenceables, which are anything able to be referenced through obsidian link or tag. These include
Files, headings, indexed blocks, tags, ...

I chose to use an enum instead of a trait as (1) I dislike the ergonomics with dynamic dyspatch, (2) It is sometimes necessary to differentiate between members of this abstraction, (3) it was convienient for this abstraction to hold the path of the referenceable for use in matching link names etc...

The vault struct is focused on presenting data from the obsidian vault through a good usable interface. The vault module as whole, however, is in change in interfacting with the obsidian syntax, which is where the methods on this enum are applicable. Obsidian has a specific linking style, and the methods on this enum provide a way to work with this syntax in a way that decouples the interpretation from other modules. The most common one method is the `is_reference` which tells if a piece of text is a refence to a particular referenceable (which is implemented differently for each type of referenceable). As a whole, this provides an abstraction around interpreting obsidian syntax; when obsidian updates syntax, code here changes and not in other places; when new referenceables are added and code is needed to interpret/match its links, code here changes and not elsewhere.
*/
pub enum Referenceable<'a> {
    File(&'a PathBuf, &'a MDFile),
    Heading(&'a PathBuf, &'a MDHeading),
    IndexedBlock(&'a PathBuf, &'a MDIndexedBlock),
    Tag(&'a PathBuf, &'a MDTag),
    Footnote(&'a PathBuf, &'a MDFootnote),
    // TODO: Get rid of useless path here
    UnresovledFile(PathBuf, &'a String),
    UnresolvedHeading(PathBuf, &'a String, &'a String),
    /// full path, link path, index (without ^)
    UnresovledIndexedBlock(PathBuf, &'a String, &'a String),
    LinkRefDef(&'a PathBuf, &'a MDLinkReferenceDefinition),
}
impl Referenceable<'_> {
    /// Gets the generic reference name for a referenceable. This will not include any display text. If trying to determine if text is a reference of a particular referenceable, use the `is_reference` function
    pub fn get_refname(&self, root_dir: &Path) -> Option<Refname> {
        match self {
            Referenceable::File(path, _) => {
                get_obsidian_ref_path(root_dir, path).map(|string| Refname {
                    full_refname: string.to_owned(),
                    path: string.to_owned().into(),
                    ..Default::default()
                })
            }

            Referenceable::Heading(path, heading) => get_obsidian_ref_path(root_dir, path)
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

            Referenceable::IndexedBlock(path, index) => get_obsidian_ref_path(root_dir, path)
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

            Referenceable::UnresovledFile(_, path) => Some(Refname {
                full_refname: path.to_string(),
                path: Some(path.to_string()),
                ..Default::default()
            }),

            Referenceable::UnresovledIndexedBlock(_, path, index) => {
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
                WikiFileLink(_) => false,
                WikiHeadingLink(_, _, _) => false,
                WikiIndexedBlockLink(_, _, _) => false,
                MDHeadingLink(_, _, _) => false,
                MDIndexedBlockLink(_, _, _) => false,
                LinkRef(_) => false,
            },
            Referenceable::File(..) | Referenceable::UnresovledFile(..) => match reference {
                WikiFileLink(ReferenceData {
                    reference_text: file_ref_text,
                    ..
                })
                | WikiHeadingLink(.., file_ref_text, _)
                | WikiIndexedBlockLink(.., file_ref_text, _)
                | MDFileLink(ReferenceData {
                    reference_text: file_ref_text,
                    ..
                })
                | MDHeadingLink(.., file_ref_text, _)
                | MDIndexedBlockLink(.., file_ref_text, _) => {
                    matches_path_or_file(file_ref_text, self.get_refname(root_dir))
                }
                Tag(_) => false,
                Footnote(_) => false,
                LinkRef(_) => false,
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
            Referenceable::UnresovledIndexedBlock(path, ..) => path,
            Referenceable::UnresovledFile(path, ..) => path,
            Referenceable::UnresolvedHeading(path, ..) => path,
            Referenceable::LinkRefDef(path, ..) => path,
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
            Referenceable::UnresovledFile(..)
            | Referenceable::UnresolvedHeading(..)
            | Referenceable::UnresovledIndexedBlock(..) => None,
        }
    }

    pub fn is_unresolved(&self) -> bool {
        matches!(
            self,
            Referenceable::UnresolvedHeading(..)
                | Referenceable::UnresovledFile(..)
                | Referenceable::UnresovledIndexedBlock(..)
        )
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
