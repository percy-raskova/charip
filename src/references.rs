use std::path::Path;

use itertools::Itertools;
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::vault::{Referenceable, Vault};

pub fn references(vault: &Vault, cursor_position: Position, path: &Path) -> Option<Vec<Location>> {
    let references = match (
        vault.select_referenceable_at_position(path, cursor_position),
        vault.select_reference_at_position(path, cursor_position),
    ) {
        (Some(referenceable @ Referenceable::Tag(..)), Some(_)) | (Some(referenceable), None) => {
            vault.select_references_for_referenceable(&referenceable)
        }
        (_, Some(reference)) => {
            let referenceables = vault.select_referenceables_for_reference(reference, path);
            referenceables
                .into_iter()
                .flat_map(|referenceable| vault.select_references_for_referenceable(&referenceable))
                .collect_vec()
        }
        (None, None) => return None,
    };

    Some(
        references
            .into_iter()
            .filter_map(|link| {
                Url::from_file_path(link.0)
                    .map(|good| Location {
                        uri: good,
                        range: *link.1.data().range, // TODO: Why can't I use .into() here?
                    })
                    .ok()
            })
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::test_utils::create_test_vault_dir;
    use std::fs;

    /// Test: Find all references to a file from multiple source files.
    /// When cursor is on a file (or its link), find all files that link to it.
    #[test]
    fn test_references_to_file() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file
        fs::write(
            vault_dir.join("target.md"),
            "# Target Document\n\nContent here.",
        )
        .unwrap();

        // Create multiple source files that link to target
        fs::write(
            vault_dir.join("source1.md"),
            "# Source 1\n\nSee [link](target) for more.",
        )
        .unwrap();
        fs::write(
            vault_dir.join("source2.md"),
            "# Source 2\n\nAlso see [reference](target) here.",
        )
        .unwrap();
        fs::write(
            vault_dir.join("unrelated.md"),
            "# Unrelated\n\nNo links here.",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on plain text (not on heading) to select the file itself
        // Line 2 "Content here." - cursor at character 5
        let cursor_position = Position {
            line: 2,
            character: 5,
        };
        let target_path = vault_dir.join("target.md");

        let result = references(&vault, cursor_position, &target_path);

        assert!(result.is_some(), "Should find references to the file");
        let locations = result.unwrap();
        assert_eq!(
            locations.len(),
            2,
            "Should find 2 references from source files"
        );

        // Verify both source files are in the results
        let uris: Vec<String> = locations.iter().map(|l| l.uri.to_string()).collect();
        assert!(
            uris.iter().any(|u| u.contains("source1.md")),
            "Should include source1.md"
        );
        assert!(
            uris.iter().any(|u| u.contains("source2.md")),
            "Should include source2.md"
        );
    }

    /// Test: Find all references to a heading from files that link to it.
    /// This test verifies that when cursor is on a heading, we can find all links to that heading.
    #[test]
    fn test_references_to_heading() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file with heading
        // Note: Heading links use the heading text for matching
        fs::write(
            vault_dir.join("target.md"),
            "# Main Title\n\n## Details\n\nContent here.",
        )
        .unwrap();

        // Create source file linking to the heading with matching name
        fs::write(
            vault_dir.join("source.md"),
            "# Source\n\nSee [info](target#Details) for more.",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the "## Details" heading (line 2, character 3 is inside the heading)
        // Heading range should include "## Details"
        let cursor_position = Position {
            line: 2,
            character: 3,
        };
        let target_path = vault_dir.join("target.md");

        let result = references(&vault, cursor_position, &target_path);

        assert!(result.is_some(), "Should find references to the heading");
        let locations = result.unwrap();
        assert!(
            !locations.is_empty(),
            "Should find at least 1 reference to heading"
        );

        // Verify source.md is in the results
        let uris: Vec<String> = locations.iter().map(|l| l.uri.to_string()).collect();
        assert!(
            uris.iter().any(|u| u.contains("source.md")),
            "Should include source.md"
        );
    }

    /// Test: Tags are parsed as referenceables but Reference::Tag construction is not yet implemented.
    ///
    /// CURRENT BEHAVIOR: Tags exist as MDTag referenceables but are NOT extracted as Reference::Tag,
    /// so "find all references to a tag" returns empty results. This test documents the current
    /// limitation while verifying that tags ARE correctly parsed as referenceables.
    ///
    /// FUTURE: When Reference::Tag construction is implemented, this test should be updated
    /// to verify that all usages of #tag across files are found.
    #[test]
    fn test_references_to_tag_current_behavior() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create files with the same tag
        fs::write(vault_dir.join("file1.md"), "# File 1\n\n#project tag here.").unwrap();
        fs::write(vault_dir.join("file2.md"), "# File 2\n\n#project here too.").unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Verify tags ARE parsed as referenceables (this part works)
        let file_path = vault_dir.join("file1.md");
        let referenceables = vault.select_referenceable_nodes(Some(&file_path));
        let tag_refs: Vec<_> = referenceables
            .iter()
            .filter(|r| matches!(r, Referenceable::Tag(..)))
            .collect();
        assert_eq!(tag_refs.len(), 1, "Tag should be parsed as referenceable");

        // Position cursor on the tag
        let cursor_position = Position {
            line: 2,
            character: 2,
        };

        // Verify tag IS selected at cursor position
        let at_pos = vault.select_referenceable_at_position(&file_path, cursor_position);
        assert!(
            matches!(at_pos, Some(Referenceable::Tag(..))),
            "Tag should be selected at cursor position"
        );

        // Current behavior: references returns empty because Reference::Tag is not constructed
        // (see vault/mod.rs:721 - Tag marked as dead_code, "matched but not currently constructed")
        let result = references(&vault, cursor_position, &file_path);
        assert!(
            result.is_some(),
            "Should return Some (empty is ok for current behavior)"
        );

        // Document current limitation: returns 0 references because tags aren't extracted as References
        let locations = result.unwrap();
        assert_eq!(
            locations.len(),
            0,
            "Current behavior: Tag references not yet implemented, returns empty"
        );
    }

    /// Test: Find all references to a MyST anchor.
    #[test]
    fn test_references_to_myst_anchor() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with MyST anchor
        fs::write(
            vault_dir.join("target.md"),
            "(my-section)=\n# Section Title\n\nContent.",
        )
        .unwrap();

        // Create files with {ref} roles pointing to the anchor
        fs::write(
            vault_dir.join("source1.md"),
            "# Source 1\n\nSee {ref}`my-section` for info.",
        )
        .unwrap();
        fs::write(
            vault_dir.join("source2.md"),
            "# Source 2\n\nAlso {ref}`my-section` here.",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the anchor definition in target.md
        let cursor_position = Position {
            line: 0,
            character: 5,
        };
        let target_path = vault_dir.join("target.md");

        let result = references(&vault, cursor_position, &target_path);

        assert!(
            result.is_some(),
            "Should find references to the MyST anchor"
        );
        let locations = result.unwrap();
        assert_eq!(
            locations.len(),
            2,
            "Should find 2 references to my-section anchor"
        );

        // Verify both source files are in results
        let uris: Vec<String> = locations.iter().map(|l| l.uri.to_string()).collect();
        assert!(
            uris.iter().any(|u| u.contains("source1.md")),
            "Should include source1.md"
        );
        assert!(
            uris.iter().any(|u| u.contains("source2.md")),
            "Should include source2.md"
        );
    }

    /// Test: References returns None when cursor is not on a referenceable.
    #[test]
    fn test_references_no_referenceable_at_cursor() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create a file with plain text
        fs::write(
            vault_dir.join("plain.md"),
            "# Heading\n\nJust some plain text here.",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor in the middle of plain text (not on heading or reference)
        let cursor_position = Position {
            line: 2,
            character: 10,
        };
        let file_path = vault_dir.join("plain.md");

        let result = references(&vault, cursor_position, &file_path);

        // When cursor is on plain text, no references should be found
        // This may return None or empty vec depending on implementation
        if let Some(locations) = result {
            // Empty results are acceptable
            assert!(
                locations.is_empty(),
                "Should not find references for plain text"
            );
        }
        // None is also acceptable
    }

    /// Test: References from cursor on a link finds all references to its target.
    #[test]
    fn test_references_from_link_cursor() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file
        fs::write(vault_dir.join("target.md"), "# Target\n\nTarget content.").unwrap();

        // Create source files
        fs::write(
            vault_dir.join("source1.md"),
            "# Source 1\n\nLink to [target](target).",
        )
        .unwrap();
        fs::write(
            vault_dir.join("source2.md"),
            "# Source 2\n\nAnother [target link](target).",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the link in source1.md (inside the link text/target)
        let cursor_position = Position {
            line: 2,
            character: 15,
        };
        let source_path = vault_dir.join("source1.md");

        let result = references(&vault, cursor_position, &source_path);

        assert!(
            result.is_some(),
            "Should find references from link cursor position"
        );
        let locations = result.unwrap();
        // Should find both links to the target
        assert!(
            locations.len() >= 2,
            "Should find at least 2 references to target"
        );
    }
}
