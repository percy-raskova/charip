//! AST-based reference extraction using markdown-rs.
//!
//! This module provides an alternative to regex-based parsing,
//! extracting references by traversing the markdown AST.

use markdown::{to_mdast, ParseOptions, mdast::Node};
use ropey::Rope;
use super::{Reference, ReferenceData, MyRange};

/// Extract all references from markdown text using AST parsing.
///
/// This is the AST-based replacement for `Reference::new()`.
pub fn extract_references_from_ast<'a>(
    text: &'a str,
    file_name: &'a str,
) -> impl Iterator<Item = Reference> + 'a {
    // TODO: Implement in Phase 3
    std::iter::empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder() {
        let refs: Vec<_> = extract_references_from_ast("test", "file").collect();
        assert!(refs.is_empty()); // Placeholder until implementation
    }
}
