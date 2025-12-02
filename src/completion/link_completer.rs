use std::{collections::HashSet, iter::once, path::Path};

use chrono::{Duration, NaiveDate};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionTextEdit,
    Documentation, InsertTextFormat, Position, Range, TextEdit,
};

use crate::{
    completion::util::check_in_code_block,
    config::Settings,
    ui::preview_referenceable,
    vault::{graph::DocumentNode, MDHeading, Reference, Referenceable, Vault},
};

use super::{
    matcher::{fuzzy_match_completions, Matchable},
    Completable, Completer, Context,
};

/// Range on a single line; assumes that the line number is known.
type LineRange = std::ops::Range<usize>;

pub struct MarkdownLinkCompleter<'a> {
    /// The display text of a link to be completed
    pub display: (String, LineRange),
    /// the filepath of the markdown link to be completed
    pub path: (String, LineRange),
    /// the infile ref; the range is the whole span of the infile ref. (including the ^ for Block refs)
    pub infile_ref: Option<(PartialInfileRef, LineRange)>,

    pub full_range: LineRange,
    pub line_nr: usize,
    pub position: Position,
    pub vault: &'a Vault,
    pub context_path: &'a Path,
    pub settings: &'a Settings,
}

pub trait LinkCompleter<'a>: Completer<'a> {
    fn settings(&self) -> &'a Settings;
    fn completion_text_edit(&self, display: Option<&str>, refname: &str) -> CompletionTextEdit;
    fn entered_refname(&self) -> String;
    fn vault(&self) -> &'a Vault;
    fn position(&self) -> Position;
    fn path(&self) -> &'a Path;
    fn link_completions(&self) -> Vec<LinkCompletion<'a>>
    where
        Self: Sync,
    {
        let referenceables = self.vault().select_referenceable_nodes(None);

        let position = self.position();

        let unresolved_under_cursor = self
            .vault()
            .select_reference_at_position(self.path(), position)
            .map(|reference| {
                self.vault()
                    .select_referenceables_for_reference(reference, self.path())
            })
            .into_iter()
            .flatten()
            .find(|referenceable| referenceable.is_unresolved());

        let single_unresolved_under_cursor = unresolved_under_cursor.and_then(|referenceable| {
            let ref_count = self
                .vault()
                .select_references_for_referenceable(&referenceable)
                .len();

            if ref_count == 1 {
                Some(referenceable)
            } else {
                None
            }
        });

        let heading_completions = self.settings().heading_completions;

        // Get and filter referenceables
        let completions = referenceables
            .into_par_iter()
            .filter(|referenceable| Some(referenceable) != single_unresolved_under_cursor.as_ref())
            .filter(|referenceable| {
                heading_completions
                    || !matches!(
                        referenceable,
                        Referenceable::Heading(..) | Referenceable::UnresolvedHeading(..)
                    )
            })
            .flat_map(|referenceable| {
                LinkCompletion::new(referenceable.clone(), self)
                    .into_iter()
                    .par_bridge()
            })
            .flatten()
            .collect::<Vec<_>>();

        // TODO: This could be slow
        let refnames = completions
            .par_iter()
            .map(|completion| completion.refname())
            .collect::<HashSet<_>>();

        // Get daily notes for convienience
        let today = chrono::Local::now().date_naive();
        let days = (-7..=7)
            .flat_map(|i| Some(today + Duration::try_days(i)?))
            .flat_map(|date| MDDailyNote::from_date(date, self))
            .filter(|date| !refnames.contains(&date.ref_name))
            .map(LinkCompletion::DailyNote);

        completions.into_iter().chain(days).collect::<Vec<_>>()
    }
}

impl<'a> LinkCompleter<'a> for MarkdownLinkCompleter<'a> {
    fn settings(&self) -> &'a Settings {
        self.settings
    }

    fn path(&self) -> &'a Path {
        self.context_path
    }
    fn position(&self) -> Position {
        self.position
    }

    fn vault(&self) -> &'a Vault {
        self.vault
    }

    fn entered_refname(&self) -> String {
        format!(
            "{}{}",
            self.path.0,
            self.infile_ref
                .as_ref()
                .map(|infile| infile.0.to_string())
                .unwrap_or("".to_string())
        )
    }

    /// Will add <$1> to the refname if it contains spaces
    fn completion_text_edit(&self, display: Option<&str>, refname: &str) -> CompletionTextEdit {
        let ext = if self.settings().include_md_extension_md_link {
            ".md"
        } else {
            ""
        };

        let format_link = |name: &str, suffix: &str| {
            if name.contains(' ') || suffix.contains(' ') {
                format!("<{}{}{}>", name, ext, suffix)
            } else {
                format!("{}{}{}", name, ext, suffix)
            }
        };

        // Handle block links foobar#^123 -> foobar.md#^123
        let link_ref_text = if let Some(pos) = refname.find("#^") {
            let (name, suffix) = refname.split_at(pos);

            if self.settings().link_filenames_only {
                format_link(name, "")
            } else {
                format_link(name, suffix)
            }
        // Handle headings links foobar#myheading -> foobar.md#myheading
        } else if let Some(pos) = refname.find('#') {
            let (name, suffix) = refname.split_at(pos);

            if self.settings().link_filenames_only {
                format_link(name, "")
            } else {
                format_link(name, suffix)
            }
        } else {
            // default case foobar -> foobar.md
            format_link(refname, "")
        };

        CompletionTextEdit::Edit(TextEdit {
            range: Range {
                start: Position {
                    line: self.line_nr as u32,
                    character: self.full_range.start as u32,
                },
                end: Position {
                    line: self.line_nr as u32,
                    character: self.full_range.end as u32,
                },
            },
            new_text: format!("[{}]({})", display.unwrap_or(""), link_ref_text),
        })
    }
}

impl<'a> Completer<'a> for MarkdownLinkCompleter<'a> {
    fn construct(context: Context<'a>, line: usize, character: usize) -> Option<Self>
    where
        Self: Sized,
    {
        if !context.settings.references_in_codeblocks
            && check_in_code_block(&context, line, character)
        {
            return None;
        }

        let Context { vault, path, .. } = context;

        let line_chars = vault.select_line(path, line as isize)?;
        let line_to_cursor = line_chars.get(0..character)?;

        static PARTIAL_MDLINK_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\[(?<display>[^\[\]\(\)]*)\]\((?<path>[^\[\]\(\)\#]+)?(\#(?<infileref>[^\[\]\(\)]*))?$").unwrap()
        }); // [display](relativePath)

        let line_string_to_cursor = String::from_iter(line_to_cursor);

        let captures = PARTIAL_MDLINK_REGEX.captures(&line_string_to_cursor)?;

        let (full, display, reftext, infileref) = (
            captures.get(0)?,
            captures.name("display")?,
            captures.name("path"),
            captures.name("infileref"),
        );

        let line_string = String::from_iter(&line_chars);

        let file_name = match context.path.file_stem() {
            Some(stem) => stem.to_string_lossy(),
            None => return None, // Invalid path, can't complete
        };
        let reference_under_cursor = Reference::new(&line_string, &file_name).find(|reference| {
            reference.range.start.character <= character as u32
                && reference.range.end.character >= character as u32
        });

        let full_range = match reference_under_cursor {
            Some(
                reference @ (Reference::MDFileLink(..)
                | Reference::MDHeadingLink(..)
                | Reference::MDIndexedBlockLink(..)),
            ) => reference.range.start.character as usize..reference.range.end.character as usize,
            None if line_chars.get(character) == Some(&')') => {
                full.range().start..full.range().end + 1
            }
            _ => full.range(),
        };

        let partial_infileref = infileref.map(|infileref| {
            let chars = infileref.as_str().chars().collect::<Vec<char>>();

            let range = infileref.range();

            match chars.as_slice() {
                ['^', rest @ ..] => (PartialInfileRef::BlockRef(String::from_iter(rest)), range),
                rest => (PartialInfileRef::HeadingRef(String::from_iter(rest)), range),
            }
        });

        let partial = Some(MarkdownLinkCompleter {
            path: (
                reftext
                    .map(|it| it.as_str().to_string())
                    .unwrap_or(file_name.to_string()),
                reftext
                    .map(|it| it.range())
                    .unwrap_or(character - 1..character - 1),
            ), // range shouldn't matter if no path specified.
            display: (display.as_str().to_string(), display.range()),
            infile_ref: partial_infileref,
            full_range,
            line_nr: line,
            position: Position {
                line: line as u32,
                character: character as u32,
            },
            vault,
            context_path: context.path,
            settings: context.settings,
        });

        partial
    }

    fn completions(&self) -> Vec<impl Completable<'a, MarkdownLinkCompleter<'a>>> {
        let filter_text = format!(
            "{}{}",
            self.path.0,
            self.infile_ref
                .clone()
                .map(|(infile, _)| format!("#{}", infile.completion_string()))
                .unwrap_or("".to_string())
        );

        let link_completions = self.link_completions();

        let matches =
            fuzzy_match_completions(&filter_text, link_completions, &self.settings.case_matching);

        matches
    }

    /// The completions refname
    type FilterParams = &'a str;

    fn completion_filter_text(&self, params: Self::FilterParams) -> String {
        let filter_text = format!("[{}]({}", self.display.0, params);

        filter_text
    }
}

#[derive(Debug, Clone)]
pub enum PartialInfileRef {
    HeadingRef(String),
    /// The partial reference to a block, not including the ^ index
    BlockRef(String),
}

impl std::fmt::Display for PartialInfileRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HeadingRef(string) => write!(f, "{}", string),
            Self::BlockRef(string) => write!(f, "^{}", string),
        }
    }
}

impl PartialInfileRef {
    fn completion_string(&self) -> String {
        match self {
            PartialInfileRef::HeadingRef(s) => s.to_string(),
            PartialInfileRef::BlockRef(s) => format!("^{}", s),
        }
    }
}

#[derive(Debug, Clone)]
pub enum LinkCompletion<'a> {
    File {
        doc_node: &'a DocumentNode,
        match_string: String,
        referenceable: Referenceable<'a>,
    },
    Alias {
        filename: &'a str,
        match_string: &'a str,
        referenceable: Referenceable<'a>,
    },
    Heading {
        heading: &'a MDHeading,
        match_string: String,
        referenceable: Referenceable<'a>,
    },
    Block {
        match_string: String,
        referenceable: Referenceable<'a>,
    },
    Unresolved {
        match_string: String,
        /// Infile ref includes all after #, including ^
        infile_ref: Option<String>,
        referenceable: Referenceable<'a>,
    },
    DailyNote(MDDailyNote<'a>),
}

use LinkCompletion::*;

impl LinkCompletion<'_> {
    fn new<'a>(
        referenceable: Referenceable<'a>,
        completer: &impl LinkCompleter<'a>,
    ) -> Option<Vec<LinkCompletion<'a>>> {
        if let Some(daily) = MDDailyNote::from_referenceable(referenceable.clone(), completer) {
            Some(vec![DailyNote(daily)])
        } else {
            match referenceable {
                Referenceable::File(_, doc_node) => Some(
                    once(File {
                        doc_node,
                        match_string: doc_node.file_name()?.to_string(),
                        referenceable: referenceable.clone(),
                    })
                    .chain(
                        doc_node
                            .metadata
                            .iter()
                            .flat_map(|it| it.aliases())
                            .flat_map(|alias| {
                                Some(Alias {
                                    filename: doc_node.file_name()?,
                                    match_string: alias,
                                    referenceable: referenceable.clone(),
                                })
                            }),
                    )
                    .collect(),
                ),
                Referenceable::Heading(path, mdheading) => Some(
                    once(Heading {
                        heading: mdheading,
                        match_string: format!(
                            "{}#{}",
                            path.file_stem()?.to_str()?,
                            mdheading.heading_text
                        ),
                        referenceable,
                    })
                    .collect(),
                ),
                Referenceable::IndexedBlock(path, indexed) => Some(
                    once(Block {
                        match_string: format!("{}#^{}", path.file_stem()?.to_str()?, indexed.index),
                        referenceable,
                    })
                    .collect(),
                ),
                Referenceable::UnresolvedFile(_, file) => Some(
                    once(Unresolved {
                        match_string: file.clone(),
                        infile_ref: None,
                        referenceable,
                    })
                    .collect(),
                ),
                Referenceable::UnresolvedHeading(_, s1, s2) => Some(
                    once(Unresolved {
                        match_string: format!("{}#{}", s1, s2),
                        infile_ref: Some(s2.clone()),
                        referenceable,
                    })
                    .collect(),
                ),
                Referenceable::UnresolvedIndexedBlock(_, s1, s2) => Some(
                    once(Unresolved {
                        match_string: format!("{}#^{}", s1, s2),
                        infile_ref: Some(format!("^{}", s2)),
                        referenceable,
                    })
                    .collect(),
                ),
                _ => None,
            }
        }
    }

    fn default_completion<'a>(
        &self,
        text_edit: CompletionTextEdit,
        filter_text: &str,
        completer: &impl LinkCompleter<'a>,
    ) -> CompletionItem {
        let vault = completer.vault();
        let referenceable = match self {
            Self::File { referenceable, .. }
            | Self::Heading { referenceable, .. }
            | Self::Block { referenceable, .. }
            | Self::Unresolved { referenceable, .. }
            | Self::Alias { referenceable, .. } => referenceable.to_owned(),
            Self::DailyNote(daily) => daily.referenceable(completer),
        };

        let label = self.match_string();

        CompletionItem {
            label: label.to_string(),
            kind: Some(match self {
                Self::File { .. } => CompletionItemKind::FILE,
                Self::Heading { .. } | Self::Block { .. } => CompletionItemKind::REFERENCE,
                Self::Unresolved {
                    match_string: _,
                    infile_ref: _,
                    ..
                } => CompletionItemKind::KEYWORD,
                Self::Alias { .. } => CompletionItemKind::ENUM,
                Self::DailyNote { .. } => CompletionItemKind::EVENT,
            }),
            label_details: match self {
                Self::Unresolved {
                    match_string: _,
                    infile_ref: _,
                    ..
                } => Some(CompletionItemLabelDetails {
                    detail: Some("Unresolved".into()),
                    description: None,
                }),
                Alias { filename, .. } => Some(CompletionItemLabelDetails {
                    detail: Some(format!("Alias: {}.md", filename)),
                    description: None,
                }),
                File { .. } => None,
                Heading { .. } => None,
                Block { .. } => None,
                DailyNote(_) => None,
            },
            text_edit: Some(text_edit),
            preselect: Some(match self {
                Self::DailyNote(daily) => {
                    daily.relative_name(completer) == Some(completer.entered_refname())
                }
                link_completion => link_completion.refname() == completer.entered_refname(),
            }),
            filter_text: Some(filter_text.to_string()),
            documentation: preview_referenceable(vault, &referenceable)
                .map(Documentation::MarkupContent),
            ..Default::default()
        }
    }

    /// Refname to be inserted into the document
    fn refname(&self) -> String {
        match self {
            Self::DailyNote(MDDailyNote { ref_name, .. }) => ref_name.to_string(),
            File { match_string, .. }
            | Heading { match_string, .. }
            | Block { match_string, .. }
            | Unresolved { match_string, .. } => match_string.to_string(),
            Alias { filename, .. } => filename.to_string(),
        }
    }
}

impl<'a> Completable<'a, MarkdownLinkCompleter<'a>> for LinkCompletion<'a> {
    fn completions(
        &self,
        markdown_link_completer: &MarkdownLinkCompleter<'a>,
    ) -> Option<CompletionItem> {
        let refname = self.refname();
        let match_string = self.match_string();

        let display = &markdown_link_completer.display;

        let link_display_text = match self {
            File {
                doc_node: _,
                match_string: _,
                ..
            }
            | Self::Block {
                match_string: _, ..
            } => None,
            Self::Alias { match_string, .. } => Some(match_string.to_string()),
            Self::DailyNote(daily) => daily.relative_name(markdown_link_completer),
            Self::Heading {
                heading,
                match_string: _,
                ..
            } => Some(heading.heading_text.to_string()),
            Self::Unresolved {
                match_string: _,
                infile_ref,
                ..
            } => infile_ref.clone(),
        };

        let binding = (display.0.as_str(), link_display_text);
        let link_display_text = match binding {
            ("", Some(ref infile)) => infile,
            // Get the first heading of the file, if possible.
            ("", None) if markdown_link_completer.settings().title_headings => match self {
                Self::File { doc_node, .. } => doc_node
                    .headings
                    .first()
                    .map(|heading| heading.heading_text.as_str())
                    .unwrap_or(""),
                Self::Alias {
                    match_string: alias,
                    ..
                } => alias,
                _ => "",
            },
            (display, _) => display,
        };

        let link_display_text = format!("${{1:{}}}", link_display_text,);

        let text_edit =
            markdown_link_completer.completion_text_edit(Some(&link_display_text), &refname);

        let filter_text = markdown_link_completer.completion_filter_text(match_string); // TODO: abstract into default_completion

        Some(CompletionItem {
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..self.default_completion(text_edit, &filter_text, markdown_link_completer)
        })
    }
}

impl Matchable for LinkCompletion<'_> {
    /// The string used for fuzzy matching
    fn match_string(&self) -> &str {
        match self {
            File {
                doc_node: _,
                match_string,
                ..
            }
            | Heading {
                heading: _,
                match_string,
                ..
            }
            | Block { match_string, .. }
            | Unresolved { match_string, .. }
            | DailyNote(MDDailyNote { match_string, .. }) => match_string,
            Alias { match_string, .. } => match_string,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MDDailyNote<'a> {
    match_string: String,
    ref_name: String,
    real_referenceaable: Option<Referenceable<'a>>,
}

impl MDDailyNote<'_> {
    pub fn relative_name<'a>(&self, completer: &impl LinkCompleter<'a>) -> Option<String> {
        let self_date = self.get_self_date(completer)?;

        Self::relative_date_string(self_date)
    }

    pub fn get_self_date<'a>(&self, completer: &impl LinkCompleter<'a>) -> Option<NaiveDate> {
        let dailynote_format = &completer.settings().dailynote;

        chrono::NaiveDate::parse_from_str(&self.ref_name, dailynote_format).ok()
    }

    fn relative_date_string(date: NaiveDate) -> Option<String> {
        let today = chrono::Local::now().date_naive();

        if today == date {
            Some("today".to_string())
        } else {
            match (date - today).num_days() {
                1 => Some("tomorrow".to_string()),
                2..=7 => Some(format!("next {}", date.format("%A"))),
                -1 => Some("yesterday".to_string()),
                -7..=-1 => Some(format!("last {}", date.format("%A"))),
                _ => None,
            }
        }
    }

    /// The refname used for fuzzy matching a completion - not the actual inserted text
    fn from_referenceable<'a>(
        referenceable: Referenceable<'a>,
        completer: &impl LinkCompleter<'a>,
    ) -> Option<MDDailyNote<'a>> {
        let (filerefname, filter_refname) = (match referenceable {
            Referenceable::File(&ref path, _) | Referenceable::UnresolvedFile(ref path, _) => {
                let filename = path.file_name();
                let dailynote_format = &completer.settings().dailynote;
                let (date, filename) = filename.and_then(|filename| {
                    let filename = filename.to_str()?;
                    let filename = filename.replace(".md", "");
                    Some((
                        chrono::NaiveDate::parse_from_str(&filename, dailynote_format).ok(),
                        filename,
                    ))
                })?;

                date.and_then(Self::relative_date_string)
                    .map(|thing| (filename.clone(), format!("{}: {}", thing, filename)))
            }
            _ => None,
        })?;

        Some(MDDailyNote {
            match_string: filter_refname,
            ref_name: filerefname,
            real_referenceaable: Some(referenceable),
        })
    }

    fn from_date<'a>(
        date: NaiveDate,
        completer: &impl LinkCompleter<'a>,
    ) -> Option<MDDailyNote<'a>> {
        let filerefname = date.format(&completer.settings().dailynote).to_string();
        let match_string = format!("{}: {}", Self::relative_date_string(date)?, filerefname);

        // path on unresolved file is useless
        Some(MDDailyNote {
            match_string,
            ref_name: filerefname.clone(),
            real_referenceaable: None,
        })
    }

    /// mock referenceable for kicks
    fn referenceable<'a, 'b>(&'b self, completer: &impl LinkCompleter<'a>) -> Referenceable<'b> {
        if let Some(referencaable) = &self.real_referenceaable {
            return referencaable.clone();
        }

        let mut path = completer.vault().root_dir().to_path_buf();
        path.push(format!("{}.md", self.ref_name));

        let unresolved_file = Referenceable::UnresolvedFile(path.to_path_buf(), &self.ref_name);

        unresolved_file
    }
}
