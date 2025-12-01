//! Core types for vault data structures.
//!
//! This module contains fundamental types used throughout the vault system:
//! - `HeadingLevel`: Represents the level of a Markdown heading (1-6)
//! - `MyRange`: A wrapper around LSP Range with additional utilities

use std::ops::{Deref, Range};

use ropey::Rope;
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::Position;

/// Represents a Markdown heading level (1-6).
#[derive(Eq, PartialEq, Debug, PartialOrd, Ord, Clone, Hash)]
pub struct HeadingLevel(pub usize);

impl Default for HeadingLevel {
    fn default() -> Self {
        HeadingLevel(1)
    }
}

/// A wrapper around `tower_lsp::lsp_types::Range` with additional utilities.
///
/// Provides conversion from byte offsets to LSP positions using rope-based
/// character counting.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct MyRange(pub tower_lsp::lsp_types::Range);

impl MyRange {
    /// Creates a `MyRange` from a byte offset range using rope for position calculation.
    pub fn from_range(rope: &Rope, range: Range<usize>) -> MyRange {
        // convert from byte offset to char offset
        let char_start = rope.byte_to_char(range.start);
        let char_end = rope.byte_to_char(range.end);

        let start_line = rope.char_to_line(char_start);
        let start_offset = char_start - rope.line_to_char(start_line);

        let end_line = rope.char_to_line(char_end);
        let end_offset = char_end - rope.line_to_char(end_line);

        tower_lsp::lsp_types::Range {
            start: Position {
                line: start_line as u32,
                character: start_offset as u32,
            },
            end: Position {
                line: end_line as u32,
                character: end_offset as u32,
            },
        }
        .into()
    }
}

impl std::hash::Hash for MyRange {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.start.line.hash(state);
        self.0.start.character.hash(state);
        self.0.end.character.hash(state);
        self.0.end.character.hash(state);
    }
}

impl Deref for MyRange {
    type Target = tower_lsp::lsp_types::Range;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<tower_lsp::lsp_types::Range> for MyRange {
    fn from(range: tower_lsp::lsp_types::Range) -> Self {
        MyRange(range)
    }
}

/// A parsed Markdown heading with text, position range, and heading level.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct MDHeading {
    pub heading_text: String,
    pub range: MyRange,
    pub level: HeadingLevel,
}

impl std::hash::Hash for MDHeading {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.level.hash(state);
        self.heading_text.hash(state)
    }
}

/// An indexed block in Markdown, referenced via `^index`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MDIndexedBlock {
    /// The index of the block; does not include '^'
    pub index: String,
    pub range: MyRange,
}

impl std::hash::Hash for MDIndexedBlock {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

/// A footnote definition in Markdown.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct MDFootnote {
    pub index: String,
    pub footnote_text: String,
    pub range: MyRange,
}

impl std::hash::Hash for MDFootnote {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.footnote_text.hash(state);
    }
}

/// A tag reference in Markdown (e.g., `#topic`).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MDTag {
    pub tag_ref: String,
    pub range: MyRange,
}

impl std::hash::Hash for MDTag {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.tag_ref.hash(state);
    }
}

/// A link reference definition (e.g., `[label]: url "title"`).
#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct MDLinkReferenceDefinition {
    pub link_ref_name: String,
    pub range: MyRange,
    pub url: String,
    pub title: Option<String>,
}

/// Trait for types that have a range (position span in the document).
pub trait Rangeable {
    fn range(&self) -> &MyRange;
    fn includes(&self, other: &impl Rangeable) -> bool {
        let self_range = self.range();
        let other_range = other.range();

        (self_range.start.line < other_range.start.line
            || (self_range.start.line == other_range.start.line
                && self_range.start.character <= other_range.start.character))
            && (self_range.end.line > other_range.end.line
                || (self_range.end.line == other_range.end.line
                    && self_range.end.character >= other_range.end.character))
    }

    fn includes_position(&self, position: Position) -> bool {
        let range = self.range();
        (range.start.line < position.line
            || (range.start.line == position.line && range.start.character <= position.character))
            && (range.end.line > position.line
                || (range.end.line == position.line && range.end.character >= position.character))
    }
}

impl Rangeable for MDHeading {
    fn range(&self) -> &MyRange {
        &self.range
    }
}

impl Rangeable for MDFootnote {
    fn range(&self) -> &MyRange {
        &self.range
    }
}

impl Rangeable for MDIndexedBlock {
    fn range(&self) -> &MyRange {
        &self.range
    }
}

impl Rangeable for MDTag {
    fn range(&self) -> &MyRange {
        &self.range
    }
}

impl Rangeable for MDLinkReferenceDefinition {
    fn range(&self) -> &MyRange {
        &self.range
    }
}
