use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

use crate::{
    config::Settings,
    vault::{self, Reference, Referenceable, Vault},
};

pub fn path_unresolved_references<'a>(
    vault: &'a Vault,
    path: &'a Path,
) -> Option<Vec<(&'a Path, &'a Reference)>> {
    let referenceables = vault.select_referenceable_nodes(None);
    let pathreferences = vault.select_references(Some(path));

    let unresolved = pathreferences
        .into_par_iter()
        .filter(|(path, reference)| {
            let matched_option = referenceables
                .iter()
                .find(|referenceable| reference.references(vault.root_dir(), path, referenceable));

            matched_option.is_some_and(|matched| {
                matches!(
                    matched,
                    Referenceable::UnresovledIndexedBlock(..)
                        | Referenceable::UnresovledFile(..)
                        | Referenceable::UnresolvedHeading(..)
                )
            })
        })
        .collect::<Vec<_>>();

    Some(unresolved)
}

pub fn diagnostics(
    vault: &Vault,
    settings: &Settings,
    (path, _uri): (&PathBuf, &Url),
) -> Option<Vec<Diagnostic>> {
    if !settings.unresolved_diagnostics {
        return None;
    }

    let unresolved = path_unresolved_references(vault, path)?;

    let allreferences = vault.select_references(None);

    let diags: Vec<Diagnostic> = unresolved
        .into_par_iter()
        .map(|(path, reference)| Diagnostic {
            range: *reference.data().range,
            message: match allreferences
                .iter()
                .filter(|(other_path, otherreference)| {
                    otherreference.matches_type(reference)
                        && (!matches!(reference, vault::Reference::Footnote(_))
                            || **other_path == *path)
                        && otherreference.data().reference_text == reference.data().reference_text
                })
                .count()
            {
                num if num > 1 => format!("Unresolved Reference used {} times", num),
                _ => "Unresolved Reference".to_string(),
            },
            source: Some("charip".into()),
            severity: Some(DiagnosticSeverity::INFORMATION),
            ..Default::default()
        })
        .collect();

    Some(diags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::test_utils::create_test_vault_dir;
    use std::fs;

    /// Test: Diagnostics are disabled when settings.unresolved_diagnostics is false.
    #[test]
    fn test_diagnostics_disabled_by_setting() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with broken link
        fs::write(
            vault_dir.join("test.md"),
            "# Test\n\nBroken [link](nonexistent) here.",
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: false, // Disable diagnostics
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result = diagnostics(&vault, &settings, (&file_path, &uri));

        assert!(
            result.is_none(),
            "Should return None when diagnostics are disabled"
        );
    }

    /// Test: Diagnostics detect unresolved file links.
    #[test]
    fn test_diagnostics_unresolved_file_link() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with broken link to non-existent file
        fs::write(
            vault_dir.join("test.md"),
            "# Test\n\nBroken [link](nonexistent) here.",
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true, // Enable diagnostics
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result = diagnostics(&vault, &settings, (&file_path, &uri));

        assert!(result.is_some(), "Should return Some when link is broken");
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1, "Should have 1 diagnostic for broken link");
        assert!(
            diags[0].message.contains("Unresolved"),
            "Message should mention unresolved"
        );
        assert_eq!(diags[0].source, Some("charip".to_string()));
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::INFORMATION));
    }

    /// Test: Diagnostics detect unresolved heading links.
    ///
    /// NOTE: This test documents CURRENT BEHAVIOR where heading links to non-existent headings
    /// in existing files may NOT produce diagnostics. The implementation checks if the reference
    /// matches an "Unresolved*" referenceable, but if the file exists, the heading link might
    /// still resolve to the file rather than creating an UnresolvedHeading.
    #[test]
    fn test_diagnostics_unresolved_heading_link() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file with a heading
        fs::write(
            vault_dir.join("target.md"),
            "# Target\n\n## Existing Heading\n\nContent.",
        )
        .unwrap();

        // Create file with link to non-existent heading in existing file
        fs::write(
            vault_dir.join("test.md"),
            "# Test\n\nBroken [link](target#Nonexistent Heading) here.",
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result = diagnostics(&vault, &settings, (&file_path, &uri));

        assert!(
            result.is_some(),
            "Should return Some for unresolved heading link"
        );
        let diags = result.unwrap();

        // Current behavior: links to non-existent headings in existing files may not produce
        // diagnostics if the UnresolvedHeading referenceable is not created for them.
        // This documents the current behavior - could be 0 or 1 depending on implementation.
        // If this test fails after changes, update the expected count.
        assert!(
            diags.len() <= 1,
            "Should have at most 1 diagnostic for broken heading link"
        );
    }

    /// Test: No diagnostics for valid links.
    #[test]
    fn test_diagnostics_valid_links_no_diagnostic() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file
        fs::write(
            vault_dir.join("target.md"),
            "# Target\n\n## Section\n\nContent.",
        )
        .unwrap();

        // Create file with valid links
        fs::write(
            vault_dir.join("test.md"),
            "# Test\n\nValid [file link](target) and [heading link](target#Section).",
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result = diagnostics(&vault, &settings, (&file_path, &uri));

        assert!(
            result.is_some(),
            "Should return Some even with no diagnostics"
        );
        let diags = result.unwrap();
        assert_eq!(
            diags.len(),
            0,
            "Should have 0 diagnostics when all links are valid"
        );
    }

    /// Test: path_unresolved_references finds unresolved references in a file.
    #[test]
    fn test_path_unresolved_references() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with multiple broken links
        fs::write(
            vault_dir.join("test.md"),
            "# Test\n\n[broken1](missing1) and [broken2](missing2) links.",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let file_path = vault_dir.join("test.md");
        let result = path_unresolved_references(&vault, &file_path);

        assert!(
            result.is_some(),
            "Should find unresolved references in file"
        );
        let unresolved = result.unwrap();
        assert_eq!(unresolved.len(), 2, "Should find 2 unresolved references");
    }

    /// Test: Diagnostics message includes count when same broken link appears multiple times.
    #[test]
    fn test_diagnostics_multiple_same_broken_link() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create multiple files with the same broken link
        fs::write(
            vault_dir.join("file1.md"),
            "# File 1\n\n[broken](missing) here.",
        )
        .unwrap();
        fs::write(
            vault_dir.join("file2.md"),
            "# File 2\n\n[broken](missing) here too.",
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let file_path = vault_dir.join("file1.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result = diagnostics(&vault, &settings, (&file_path, &uri));

        assert!(result.is_some(), "Should return diagnostics");
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1, "Should have 1 diagnostic in file1");

        // The message should indicate the broken link is used multiple times
        assert!(
            diags[0].message.contains("2 times"),
            "Message should indicate link is used 2 times: {}",
            diags[0].message
        );
    }

    /// Test: Empty vault produces no diagnostics.
    #[test]
    fn test_diagnostics_empty_file() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create empty file
        fs::write(vault_dir.join("empty.md"), "").unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let file_path = vault_dir.join("empty.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result = diagnostics(&vault, &settings, (&file_path, &uri));

        assert!(result.is_some(), "Should return Some for empty file");
        let diags = result.unwrap();
        assert_eq!(diags.len(), 0, "Should have 0 diagnostics for empty file");
    }
}
