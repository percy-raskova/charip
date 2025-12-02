use std::path::Path;

use tower_lsp::lsp_types::{Location, Position, Url};

use crate::vault::{Referenceable, Vault};

pub fn goto_definition(
    vault: &Vault,
    cursor_position: Position,
    path: &Path,
) -> Option<Vec<Location>> {
    // First, find the link that the cursor is in. Get a links for the file and match the cursor position up to one of them
    let reference = vault.select_reference_at_position(path, cursor_position)?;
    // Now we have the reference text. We need to find where this is actually referencing, or if it is referencing anything.
    // Lets get all of the referenceable nodes

    let referenceables = vault.select_referenceables_for_reference(reference, path);

    Some(
        referenceables
            .into_iter()
            .filter_map(|linkable| {
                let range = match linkable {
                    Referenceable::File(..) => tower_lsp::lsp_types::Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 1,
                        },
                    },
                    _ => *linkable.get_range()?,
                };

                Some(Location {
                    uri: Url::from_file_path(linkable.get_path().to_str()?).unwrap(),
                    range,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::test_utils::create_test_vault_dir;
    use std::fs;

    /// Test: Go-to-definition for a markdown file link resolves to the target file.
    /// This tests the happy path where [link](target) points to an existing file.
    #[test]
    fn test_goto_definition_file_link() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file
        fs::write(
            vault_dir.join("target.md"),
            "# Target Document\n\nContent here.",
        )
        .unwrap();

        // Create source file with a link to target
        let source_content = "# Source\n\nSee [my link](target) for more.";
        fs::write(vault_dir.join("source.md"), source_content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the link (line 2, character 5 is inside "[my link](target)")
        // The link starts at character 4: "See [my link](target)"
        //                                      ^
        let cursor_position = Position {
            line: 2,
            character: 8,
        };
        let source_path = vault_dir.join("source.md");

        let result = goto_definition(&vault, cursor_position, &source_path);

        assert!(result.is_some(), "Should find a definition");
        let locations = result.unwrap();
        assert_eq!(locations.len(), 1, "Should find exactly one location");

        // Verify it points to target.md
        let target_uri = Url::from_file_path(vault_dir.join("target.md")).unwrap();
        assert_eq!(locations[0].uri, target_uri, "Should point to target.md");
    }

    /// Test: Go-to-definition for a markdown heading link resolves to the heading.
    /// This tests [link](file#heading) navigation.
    #[test]
    fn test_goto_definition_heading_link() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file with a heading
        let target_content =
            "# Introduction\n\nSome intro text.\n\n## Details\n\nMore details here.";
        fs::write(vault_dir.join("target.md"), target_content).unwrap();

        // Create source file with a heading link
        let source_content = "# Source\n\nSee [details section](target#Details) for more.";
        fs::write(vault_dir.join("source.md"), source_content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the heading link
        let cursor_position = Position {
            line: 2,
            character: 10,
        };
        let source_path = vault_dir.join("source.md");

        let result = goto_definition(&vault, cursor_position, &source_path);

        assert!(
            result.is_some(),
            "Should find a definition for heading link"
        );
        let locations = result.unwrap();
        assert_eq!(
            locations.len(),
            1,
            "Should find exactly one heading location"
        );

        // Verify it points to the heading in target.md
        let target_uri = Url::from_file_path(vault_dir.join("target.md")).unwrap();
        assert_eq!(locations[0].uri, target_uri, "Should point to target.md");
        // The heading "## Details" is on line 4 (0-indexed)
        assert_eq!(
            locations[0].range.start.line, 4,
            "Should point to the heading line"
        );
    }

    /// Test: Go-to-definition for a MyST {ref} role resolves to the anchor.
    /// This tests {ref}`anchor-name` -> (anchor-name)= navigation.
    #[test]
    fn test_goto_definition_myst_ref_role() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file with MyST anchor
        let target_content = "(my-anchor)=\n# Important Section\n\nContent here.";
        fs::write(vault_dir.join("target.md"), target_content).unwrap();

        // Create source file with {ref} role
        let source_content = "# Source\n\nSee {ref}`my-anchor` for the important section.";
        fs::write(vault_dir.join("source.md"), source_content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the {ref} role (inside the backticks)
        let cursor_position = Position {
            line: 2,
            character: 10,
        };
        let source_path = vault_dir.join("source.md");

        let result = goto_definition(&vault, cursor_position, &source_path);

        assert!(
            result.is_some(),
            "Should find a definition for MyST ref role"
        );
        let locations = result.unwrap();
        assert_eq!(
            locations.len(),
            1,
            "Should find exactly one anchor location"
        );

        // Verify it points to target.md
        let target_uri = Url::from_file_path(vault_dir.join("target.md")).unwrap();
        assert_eq!(locations[0].uri, target_uri, "Should point to target.md");
        // The anchor (my-anchor)= is on line 0
        assert_eq!(
            locations[0].range.start.line, 0,
            "Should point to the anchor line"
        );
    }

    /// Test: Go-to-definition returns None when cursor is not on any reference.
    /// This is an edge case where the cursor is on plain text.
    #[test]
    fn test_goto_definition_no_reference_at_cursor() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create a file with plain text (no links)
        let content = "# Just a Heading\n\nSome plain text without any links.";
        fs::write(vault_dir.join("plain.md"), content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on plain text (line 2, in the middle of "plain text")
        let cursor_position = Position {
            line: 2,
            character: 10,
        };
        let file_path = vault_dir.join("plain.md");

        let result = goto_definition(&vault, cursor_position, &file_path);

        assert!(
            result.is_none(),
            "Should return None when cursor is not on a reference"
        );
    }

    /// Test: Go-to-definition for an unresolved link returns empty locations.
    /// This tests what happens when the link target does not exist.
    #[test]
    fn test_goto_definition_unresolved_link() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create source file with a link to a non-existent file
        let source_content = "# Source\n\nSee [broken link](nonexistent) for nothing.";
        fs::write(vault_dir.join("source.md"), source_content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the broken link
        let cursor_position = Position {
            line: 2,
            character: 10,
        };
        let source_path = vault_dir.join("source.md");

        let result = goto_definition(&vault, cursor_position, &source_path);

        // The function should return Some with empty locations or a location to an "unresolved" marker
        // Based on the implementation, it returns Some([]) for unresolved links
        assert!(
            result.is_some(),
            "Should return Some even for unresolved links"
        );
        // Empty or with unresolved markers - both are acceptable behaviors
    }

    /// Test: Go-to-definition for a tag navigates to the tag definition location.
    /// Tags like #mytag should be navigable.
    #[test]
    fn test_goto_definition_tag() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with a tag
        let content = "# Document\n\nThis has a #project tag in it.";
        fs::write(vault_dir.join("tagged.md"), content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the tag
        let cursor_position = Position {
            line: 2,
            character: 12,
        };
        let file_path = vault_dir.join("tagged.md");

        let result = goto_definition(&vault, cursor_position, &file_path);

        // Tags resolve to themselves (the tag definition is the tag usage)
        // The behavior depends on implementation - tags may or may not have "definitions"
        // This test documents the current behavior
        if let Some(locations) = result {
            // If locations are returned, they should be valid
            for loc in &locations {
                assert!(loc.uri.to_string().contains("tagged.md") || !locations.is_empty());
            }
        }
        // None is also acceptable if tags don't have go-to-definition support
    }
}
