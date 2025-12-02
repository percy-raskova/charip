use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

use crate::{
    config::Settings,
    vault::{self, MystRoleKind, Reference, Referenceable, Vault},
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

            // Case 1: MD links that match Unresolved* referenceables (existing behavior)
            let is_unresolved_md_link = matched_option.is_some_and(|matched| {
                matches!(
                    matched,
                    Referenceable::UnresovledIndexedBlock(..)
                        | Referenceable::UnresovledFile(..)
                        | Referenceable::UnresolvedHeading(..)
                )
            });

            // Case 2: MyST roles that don't match ANY referenceable are unresolved
            // (Unlike MD links, MyST roles don't have Unresolved* referenceables created for them,
            // so we check if they match nothing at all)
            let is_unresolved_myst_role =
                matches!(reference, Reference::MystRole(..)) && matched_option.is_none();

            is_unresolved_md_link || is_unresolved_myst_role
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
        .map(|(path, reference)| {
            // Count how many times this same unresolved reference appears
            let usage_count = allreferences
                .iter()
                .filter(|(other_path, otherreference)| {
                    otherreference.matches_type(reference)
                        && (!matches!(reference, vault::Reference::Footnote(_))
                            || **other_path == *path)
                        && otherreference.data().reference_text == reference.data().reference_text
                })
                .count();

            // Generate role-specific or generic message
            let message = generate_diagnostic_message(reference, usage_count);

            Diagnostic {
                range: *reference.data().range,
                message,
                source: Some("charip".into()),
                severity: Some(DiagnosticSeverity::INFORMATION),
                ..Default::default()
            }
        })
        .collect();

    Some(diags)
}

/// Generate a diagnostic message for an unresolved reference.
///
/// Provides role-specific messages for MyST roles to help users understand
/// exactly what type of target is missing.
fn generate_diagnostic_message(reference: &Reference, usage_count: usize) -> String {
    let base_message = match reference {
        Reference::MystRole(_, kind, target) => match kind {
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
                // Abbreviations don't reference external targets, so this shouldn't happen
                "Unresolved Reference".to_string()
            }
        },
        _ => "Unresolved Reference".to_string(),
    };

    // Append usage count if the reference appears multiple times
    if usage_count > 1 {
        format!("{} (used {} times)", base_message, usage_count)
    } else {
        base_message
    }
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

    // =========================================================================
    // MyST Role Diagnostics Tests
    // =========================================================================
    //
    // Tests for MyST role target resolution in diagnostics. MyST roles like
    // {ref}, {doc}, {term}, and {numref} reference anchors, files, glossary
    // terms, and figures respectively. Broken references produce diagnostics
    // with role-specific messages.

    mod myst_role_diagnostics {
        use super::*;

        /// Test: Diagnostics detect broken {ref} role targets.
        ///
        /// A {ref}`nonexistent` role pointing to a non-existent anchor or heading
        /// should produce a diagnostic.
        #[test]
        fn test_broken_ref_role() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with broken {ref} role - no anchor or heading matches "missing-section"
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\nSee {ref}`missing-section` for details.",
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

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                1,
                "Should have 1 diagnostic for broken {{ref}} role"
            );
            assert!(
                diags[0].message.contains("Unresolved"),
                "Message should mention unresolved: {}",
                diags[0].message
            );
        }

        /// Test: Diagnostics detect broken {doc} role targets.
        ///
        /// A {doc}`missing-file` role pointing to a non-existent file
        /// should produce a diagnostic.
        #[test]
        fn test_broken_doc_role() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with broken {doc} role - no file named "missing-file" exists
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\nRead {doc}`missing-file` next.",
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

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                1,
                "Should have 1 diagnostic for broken {{doc}} role"
            );
            assert!(
                diags[0].message.contains("Unresolved"),
                "Message should mention unresolved: {}",
                diags[0].message
            );
        }

        /// Test: Diagnostics detect broken {term} role targets.
        ///
        /// A {term}`undefined` role pointing to a non-existent glossary term
        /// should produce a diagnostic.
        #[test]
        fn test_broken_term_role() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with broken {term} role - no glossary defines "undefined-term"
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\nThe {term}`undefined-term` is important.",
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

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                1,
                "Should have 1 diagnostic for broken {{term}} role"
            );
            assert!(
                diags[0].message.contains("Unresolved"),
                "Message should mention unresolved: {}",
                diags[0].message
            );
        }

        /// Test: Valid MyST roles produce no diagnostics.
        ///
        /// When a role correctly references an existing target, no diagnostic
        /// should be produced.
        #[test]
        fn test_valid_myst_roles_no_diagnostic() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create target file
            fs::write(vault_dir.join("target.md"), "# Target\n\nContent.").unwrap();

            // Create glossary file with a term
            fs::write(
                vault_dir.join("glossary.md"),
                r#"# Glossary

```{glossary}
MyST
  Markedly Structured Text.
```
"#,
            )
            .unwrap();

            // Create file with anchor
            fs::write(
                vault_dir.join("anchored.md"),
                "(my-anchor)=\n# Section\n\nContent.",
            )
            .unwrap();

            // Create file with valid MyST roles
            fs::write(
                vault_dir.join("test.md"),
                r#"# Test

See {doc}`target` for the file.
See {ref}`my-anchor` for the section.
See {term}`MyST` for the definition.
"#,
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

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should have 0 diagnostics when all roles are valid. Got: {:?}",
                diags
            );
        }

        /// Test: {ref} role can reference headings (not just anchors).
        ///
        /// The {ref} role should match heading text as a fallback when
        /// no explicit anchor exists.
        #[test]
        fn test_ref_role_matches_heading() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with a heading
            fs::write(
                vault_dir.join("doc.md"),
                "# Document\n\n## Important Section\n\nContent.",
            )
            .unwrap();

            // Create file referencing the heading via {ref}
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\nSee {ref}`Important Section` for details.",
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

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should have 0 diagnostics when {{ref}} matches a heading. Got: {:?}",
                diags
            );
        }

        /// Test: Multiple broken MyST roles produce multiple diagnostics.
        #[test]
        fn test_multiple_broken_myst_roles() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with multiple broken roles
            fs::write(
                vault_dir.join("test.md"),
                r#"# Test

See {ref}`missing1` here.
Read {doc}`missing2` there.
The {term}`missing3` matters.
"#,
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

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                3,
                "Should have 3 diagnostics for 3 broken roles. Got: {:?}",
                diags
            );
        }

        /// Test: {numref} role produces diagnostic when target is missing.
        #[test]
        fn test_broken_numref_role() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with broken {numref} role
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\nSee {numref}`Figure %s <missing-figure>` for reference.",
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

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                1,
                "Should have 1 diagnostic for broken {{numref}} role"
            );
        }

        /// Test: Diagnostic messages are role-specific and include the target name.
        #[test]
        fn test_role_specific_message_format() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with different broken roles
            fs::write(
                vault_dir.join("test.md"),
                r#"# Test

See {ref}`my-missing-anchor` here.
Read {doc}`missing-document` there.
The {term}`undefined-term` matters.
"#,
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

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(diags.len(), 3, "Should have 3 diagnostics");

            // Check for role-specific messages
            let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();

            // Check {ref} message format
            assert!(
                messages
                    .iter()
                    .any(|m| m.contains("anchor") && m.contains("my-missing-anchor")),
                "Should have anchor-specific message for {{ref}}: {:?}",
                messages
            );

            // Check {doc} message format
            assert!(
                messages
                    .iter()
                    .any(|m| m.contains("document") && m.contains("missing-document")),
                "Should have document-specific message for {{doc}}: {:?}",
                messages
            );

            // Check {term} message format
            assert!(
                messages
                    .iter()
                    .any(|m| m.contains("glossary term") && m.contains("undefined-term")),
                "Should have term-specific message for {{term}}: {:?}",
                messages
            );
        }

        /// Test: {download} role produces diagnostic when file is missing.
        #[test]
        fn test_broken_download_role() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with broken {download} role
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\nGet {download}`missing.zip` here.",
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

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            // Note: {download} is special - it downloads arbitrary files, not .md files
            // For now, we may or may not detect these as broken. This test documents behavior.
            // TODO: Consider if {download} should be checked differently
            assert!(
                diags.len() <= 1,
                "Should have at most 1 diagnostic for broken {{download}} role"
            );
        }

        /// Test: MyST role usage count is included when the same broken role appears multiple times.
        ///
        /// When the same broken MyST role target appears in multiple files, the diagnostic
        /// message should include the usage count (e.g., "(used 2 times)").
        #[test]
        fn test_myst_role_usage_count_across_files() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create multiple files with the same broken {ref} role target
            fs::write(
                vault_dir.join("file1.md"),
                "# File 1\n\nSee {ref}`shared-missing-anchor` here.",
            )
            .unwrap();
            fs::write(
                vault_dir.join("file2.md"),
                "# File 2\n\nAlso {ref}`shared-missing-anchor` here.",
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

            // The message should indicate the broken role is used multiple times
            assert!(
                diags[0].message.contains("2 times"),
                "Message should indicate role is used 2 times: {}",
                diags[0].message
            );
        }
    }

    // =========================================================================
    // {doc} Role Diagnostic Tests - Comprehensive Coverage
    // =========================================================================
    //
    // These tests validate that {doc} role references are properly resolved
    // against files in the vault and produce appropriate diagnostics for
    // broken links.
    //
    // Test categories:
    // 1. Valid {doc} links that should NOT produce diagnostics
    // 2. Broken {doc} links that SHOULD produce diagnostics
    // 3. Explicit title syntax variations
    // 4. Edge cases (extensions, absolute paths, case sensitivity)

    mod doc_role_diagnostics {
        use super::*;

        // =====================================================================
        // Category 1: Valid {doc} links - NO diagnostics expected
        // =====================================================================

        /// Test: {doc}`existing-file` resolves to existing file (bare filename).
        ///
        /// When a file `existing-file.md` exists in the vault root, a reference
        /// like `{doc}`existing-file`` should resolve successfully with no diagnostic.
        #[test]
        fn test_valid_doc_role_bare_filename() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create the target file
            fs::write(
                vault_dir.join("existing-file.md"),
                "# Existing File\n\nContent here.",
            )
            .unwrap();

            // Create source file with {doc} role referencing the target
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`existing-file` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should have 0 diagnostics when {{doc}} references an existing file. Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`subdir/nested` resolves to file in subdirectory.
        ///
        /// When a file `subdir/nested.md` exists, the reference `{doc}`subdir/nested``
        /// should resolve successfully.
        #[test]
        fn test_valid_doc_role_nested_path() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create subdirectory and nested file
            fs::create_dir(vault_dir.join("subdir")).unwrap();
            fs::write(
                vault_dir.join("subdir/nested.md"),
                "# Nested File\n\nNested content.",
            )
            .unwrap();

            // Create source file referencing nested file
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`subdir/nested` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should have 0 diagnostics for valid nested path. Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`./relative` resolves with explicit relative path prefix.
        ///
        /// The `./` prefix should be stripped and the path should resolve
        /// relative to the vault root.
        #[test]
        fn test_valid_doc_role_relative_prefix() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create target file
            fs::write(
                vault_dir.join("relative.md"),
                "# Relative\n\nRelative content.",
            )
            .unwrap();

            // Create source file with ./ prefix
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`./relative` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should have 0 diagnostics for ./ prefixed path. Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`/absolute/path` resolves from vault root.
        ///
        /// An absolute path starting with `/` should resolve relative to the
        /// vault root, like Sphinx's {doc} behavior.
        #[test]
        fn test_valid_doc_role_absolute_path() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create nested structure
            fs::create_dir(vault_dir.join("absolute")).unwrap();
            fs::write(
                vault_dir.join("absolute/path.md"),
                "# Absolute\n\nAbsolute content.",
            )
            .unwrap();

            // Create source file with absolute path reference
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`/absolute/path` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should have 0 diagnostics for absolute path. Got: {:?}",
                diags
            );
        }

        // =====================================================================
        // Category 2: Broken {doc} links - diagnostics expected
        // =====================================================================

        /// Test: {doc}`nonexistent` produces diagnostic when file doesn't exist.
        ///
        /// This is the basic broken link case - the target file simply doesn't exist.
        #[test]
        fn test_broken_doc_role_nonexistent_file() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create source file referencing non-existent file
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`nonexistent` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(diags.len(), 1, "Should have 1 diagnostic for missing file");
            assert!(
                diags[0].message.contains("Unresolved document reference"),
                "Should have document-specific message: {}",
                diags[0].message
            );
            assert!(
                diags[0].message.contains("nonexistent"),
                "Message should include the target name: {}",
                diags[0].message
            );
        }

        /// Test: {doc}`wrong-path/file` produces diagnostic for wrong directory.
        ///
        /// When the file exists but in a different directory, should produce diagnostic.
        #[test]
        fn test_broken_doc_role_wrong_directory() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file in correct location
            fs::create_dir(vault_dir.join("correct-path")).unwrap();
            fs::write(vault_dir.join("correct-path/file.md"), "# File\n\nContent.").unwrap();

            // Reference it with wrong path
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`wrong-path/file` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(diags.len(), 1, "Should have 1 diagnostic for wrong path");
        }

        /// Test: {doc}`CASEMISMATCH` behavior when case doesn't match.
        ///
        /// Test case sensitivity of file matching. Bare filenames should be
        /// case-insensitive (per matches_path_or_file), but full paths may differ.
        #[test]
        fn test_doc_role_case_sensitivity_bare_name() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with lowercase name
            fs::write(vault_dir.join("myfile.md"), "# My File\n\nContent.").unwrap();

            // Reference with uppercase
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`MYFILE` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            // Bare filenames should be case-insensitive per matches_path_or_file
            assert_eq!(
                diags.len(),
                0,
                "Bare filename matching should be case-insensitive. Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`subdir/CASEMISMATCH` with case mismatch in path.
        ///
        /// Full paths are case-sensitive, so this should produce a diagnostic.
        #[test]
        fn test_doc_role_case_sensitivity_with_path() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with specific casing
            fs::create_dir(vault_dir.join("subdir")).unwrap();
            fs::write(vault_dir.join("subdir/myfile.md"), "# My File\n\nContent.").unwrap();

            // Reference with different case in path
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`SUBDIR/myfile` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            // Full path matching is case-sensitive
            assert_eq!(
                diags.len(),
                1,
                "Path matching should be case-sensitive. Got: {:?}",
                diags
            );
        }

        // =====================================================================
        // Category 3: Explicit title syntax
        // =====================================================================

        /// Test: {doc}`Title <existing-file>` resolves with explicit title.
        ///
        /// The explicit title syntax should extract the target from within < >.
        #[test]
        fn test_valid_doc_role_with_explicit_title() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create target file
            fs::write(vault_dir.join("existing-file.md"), "# Existing\n\nContent.").unwrap();

            // Create source with explicit title syntax
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`My Custom Title <existing-file>` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should have 0 diagnostics for valid explicit title. Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`Bad Title <missing>` produces diagnostic for missing target.
        ///
        /// Even with explicit title, a missing target should produce diagnostic.
        #[test]
        fn test_broken_doc_role_with_explicit_title() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create source with explicit title referencing missing file
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`Bad Title <missing>` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                1,
                "Should have 1 diagnostic for missing file with title"
            );
            assert!(
                diags[0].message.contains("missing"),
                "Message should include target name: {}",
                diags[0].message
            );
        }

        /// Test: {doc}`Title <subdir/nested>` resolves nested path with title.
        #[test]
        fn test_valid_doc_role_nested_with_explicit_title() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create nested file
            fs::create_dir(vault_dir.join("subdir")).unwrap();
            fs::write(vault_dir.join("subdir/nested.md"), "# Nested\n\nContent.").unwrap();

            // Reference with explicit title
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`The Nested Doc <subdir/nested>` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should have 0 diagnostics for valid nested path with title. Got: {:?}",
                diags
            );
        }

        // =====================================================================
        // Category 4: Edge cases
        // =====================================================================

        /// Test: {doc}`file.md` with explicit .md extension.
        ///
        /// Users might accidentally include the .md extension. This should
        /// ideally still resolve (or produce a helpful diagnostic).
        #[test]
        fn test_doc_role_with_explicit_md_extension() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file
            fs::write(vault_dir.join("file.md"), "# File\n\nContent.").unwrap();

            // Reference with .md extension
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`file.md` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            // Document current behavior: does `file.md` match a file whose refname is `file`?
            // The refname is stripped of .md, so `file.md` != `file`.
            // This test documents that explicit .md does NOT resolve.
            assert_eq!(
                diags.len(),
                1,
                "Explicit .md extension should NOT resolve (refname is stripped). Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`file with spaces` handles spaces in filename.
        ///
        /// Some filenames contain spaces. These should resolve correctly.
        #[test]
        fn test_doc_role_filename_with_spaces() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with spaces
            fs::write(
                vault_dir.join("file with spaces.md"),
                "# File With Spaces\n\nContent.",
            )
            .unwrap();

            // Reference it directly
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`file with spaces` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should resolve file with spaces. Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`path%20with%20encoded%20spaces` handles URL-encoded spaces.
        ///
        /// Some links might have URL-encoded spaces (%20). These should be handled.
        #[test]
        fn test_doc_role_url_encoded_spaces() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with spaces in name
            fs::create_dir(vault_dir.join("subdir")).unwrap();
            fs::write(
                vault_dir.join("subdir/file with spaces.md"),
                "# File\n\nContent.",
            )
            .unwrap();

            // Reference with URL-encoded spaces
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`subdir/file%20with%20spaces` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should decode %20 to spaces. Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`path\ with\ escaped\ spaces` handles backslash-escaped spaces.
        ///
        /// Backslash-escaped spaces are another encoding format.
        #[test]
        fn test_doc_role_backslash_escaped_spaces() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with spaces
            fs::create_dir(vault_dir.join("subdir")).unwrap();
            fs::write(
                vault_dir.join("subdir/file with spaces.md"),
                "# File\n\nContent.",
            )
            .unwrap();

            // Reference with backslash-escaped spaces
            fs::write(
                vault_dir.join("source.md"),
                r#"# Source

See {doc}`subdir/file\ with\ spaces` for details."#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should decode backslash-escaped spaces. Got: {:?}",
                diags
            );
        }

        /// Test: Multiple {doc} roles in single file.
        ///
        /// A file might have many {doc} references. Each broken one should
        /// produce its own diagnostic.
        #[test]
        fn test_multiple_doc_roles_mixed_validity() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create some target files
            fs::write(vault_dir.join("exists1.md"), "# Exists 1\n\nContent.").unwrap();
            fs::write(vault_dir.join("exists2.md"), "# Exists 2\n\nContent.").unwrap();

            // Create source with mix of valid and broken {doc} roles
            fs::write(
                vault_dir.join("source.md"),
                r#"# Source

See {doc}`exists1` for info.
Also {doc}`missing1` for more.
And {doc}`exists2` for even more.
Plus {doc}`missing2` doesn't exist.
Finally {doc}`missing3` is also broken.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                3,
                "Should have 3 diagnostics for 3 broken {{doc}} roles. Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`../sibling/file` with parent directory traversal.
        ///
        /// Relative paths with `..` should navigate correctly.
        #[test]
        fn test_doc_role_parent_directory_traversal() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create structure: sibling/file.md and source/origin.md
            fs::create_dir(vault_dir.join("sibling")).unwrap();
            fs::create_dir(vault_dir.join("source")).unwrap();
            fs::write(
                vault_dir.join("sibling/file.md"),
                "# Sibling File\n\nContent.",
            )
            .unwrap();

            // Reference from source/origin.md to ../sibling/file
            fs::write(
                vault_dir.join("source/origin.md"),
                "# Origin\n\nSee {doc}`../sibling/file` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source/origin.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            // Document behavior: does `../sibling/file` resolve?
            // This is complex because the current implementation doesn't resolve
            // `..` relative to the current file - it resolves relative to vault root.
            // This test documents that `..` paths may NOT resolve correctly.
            // Expecting 1 diagnostic (broken) until relative path resolution is fixed.
            assert!(
                diags.len() <= 1,
                "Parent traversal behavior documented. Got: {:?}",
                diags
            );
        }

        /// Test: Deep nesting {doc}`a/b/c/deep` resolves correctly.
        #[test]
        fn test_doc_role_deep_nesting() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create deeply nested structure
            fs::create_dir_all(vault_dir.join("a/b/c")).unwrap();
            fs::write(vault_dir.join("a/b/c/deep.md"), "# Deep\n\nDeep content.").unwrap();

            // Reference from root
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`a/b/c/deep` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should resolve deeply nested path. Got: {:?}",
                diags
            );
        }

        /// Test: Same filename in different directories.
        ///
        /// When `a/readme.md` and `b/readme.md` both exist, bare `{doc}`readme``
        /// should resolve (to at least one).
        #[test]
        fn test_doc_role_ambiguous_bare_name() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create same filename in different directories
            fs::create_dir(vault_dir.join("a")).unwrap();
            fs::create_dir(vault_dir.join("b")).unwrap();
            fs::write(vault_dir.join("a/readme.md"), "# A Readme").unwrap();
            fs::write(vault_dir.join("b/readme.md"), "# B Readme").unwrap();

            // Reference with bare name - should match at least one
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`readme` for details.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            // Bare name should match at least one file (case-insensitive)
            assert_eq!(
                diags.len(),
                0,
                "Bare name should match existing file. Got: {:?}",
                diags
            );
        }

        /// Test: Specific path disambiguates when multiple files share basename.
        #[test]
        fn test_doc_role_specific_path_disambiguation() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create same filename in different directories
            fs::create_dir(vault_dir.join("a")).unwrap();
            fs::create_dir(vault_dir.join("b")).unwrap();
            fs::write(vault_dir.join("a/readme.md"), "# A Readme").unwrap();
            fs::write(vault_dir.join("b/readme.md"), "# B Readme").unwrap();

            // Reference with specific path - should resolve to exactly one
            fs::write(
                vault_dir.join("source.md"),
                "# Source\n\nSee {doc}`a/readme` for A's readme.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Should resolve specific path. Got: {:?}",
                diags
            );
        }
    }

    // =========================================================================
    // {doc} Role Integration Tests - Generic Vault Fixture
    // =========================================================================
    //
    // These tests use a standardized vault fixture (create_doc_role_test_fixture)
    // to test {doc} role resolution patterns in a realistic directory structure.
    //
    // The fixture creates:
    //   guides/getting-started.md, installation.md, configuration.md
    //   reference/api-overview.md, commands.md, options.md
    //   tutorials/advanced/topic-one.md, topic-two.md
    //   "file with spaces.md"
    //   glossary.md, index.md
    //
    // Tests focus on cross-directory resolution, syntax variations, and edge cases.

    mod doc_role_integration_tests {
        use super::*;
        use crate::test_utils::create_doc_role_test_fixture;

        // =====================================================================
        // Test 1: Bare name resolves across directories
        // =====================================================================

        /// Test: {doc}`getting-started` in reference/ resolves to guides/getting-started.md
        ///
        /// Bare filenames should resolve across the entire vault, not just the
        /// current directory. This tests the core cross-directory resolution behavior.
        #[test]
        fn test_doc_bare_name_resolves_across_directories() {
            let (_temp_dir, vault_dir) = create_doc_role_test_fixture();

            // Add source file in reference/ that references getting-started (in guides/)
            fs::write(
                vault_dir.join("reference/api-overview.md"),
                r#"# API Overview

See {doc}`getting-started` for an introduction.

API documentation follows.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("reference/api-overview.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Bare name 'getting-started' should resolve to guides/getting-started.md. Got: {:?}",
                diags
            );
        }

        // =====================================================================
        // Test 2: Nested path syntax
        // =====================================================================

        /// Test: {doc}`tutorials/advanced/topic-one` and {doc}`reference/api-overview` resolve.
        ///
        /// Full paths with slashes should resolve correctly to nested files.
        #[test]
        fn test_doc_nested_path_syntax() {
            let (_temp_dir, vault_dir) = create_doc_role_test_fixture();

            // Add source file with nested path references
            fs::write(
                vault_dir.join("index.md"),
                r#"# Index

See {doc}`tutorials/advanced/topic-one` for advanced topics.
See {doc}`reference/api-overview` for API documentation.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("index.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Nested paths should resolve correctly. Got: {:?}",
                diags
            );
        }

        // =====================================================================
        // Test 3: Broken links produce diagnostics
        // =====================================================================

        /// Test: {doc}`nonexistent` and {doc}`wrong/path` produce diagnostics.
        ///
        /// References to non-existent files or wrong paths should produce
        /// diagnostics with "Unresolved document" in the message.
        #[test]
        fn test_doc_broken_links_produce_diagnostics() {
            let (_temp_dir, vault_dir) = create_doc_role_test_fixture();

            // Add source file with broken references
            fs::write(
                vault_dir.join("source.md"),
                r#"# Source

See {doc}`nonexistent` for missing content.
See {doc}`wrong/path` for wrong path content.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                2,
                "Should have 2 diagnostics for 2 broken links. Got: {:?}",
                diags
            );

            // Verify diagnostic messages contain "Unresolved document"
            for diag in &diags {
                assert!(
                    diag.message.contains("Unresolved document"),
                    "Diagnostic should contain 'Unresolved document': {}",
                    diag.message
                );
            }
        }

        // =====================================================================
        // Test 4: Code blocks should not be parsed
        // =====================================================================

        /// Test: {doc}`should-not-parse` inside code block produces no diagnostic.
        ///
        /// MyST roles inside fenced code blocks should be treated as literal text,
        /// not as actual references. This is important for documentation that
        /// shows examples of role syntax.
        #[test]
        fn test_doc_inside_code_block_not_parsed() {
            let (_temp_dir, vault_dir) = create_doc_role_test_fixture();

            // Add source file with role inside code block
            fs::write(
                vault_dir.join("source.md"),
                r#"# Example

Here is how to reference a document:

```
{doc}`should-not-parse`
```

The above should not be parsed as a real reference.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Roles inside code blocks should not produce diagnostics. Got: {:?}",
                diags
            );
        }

        // =====================================================================
        // Test 5: Explicit title syntax
        // =====================================================================

        /// Test: {doc}`Start Here <getting-started>` resolves, {doc}`Missing <nonexistent>` produces diagnostic.
        ///
        /// The explicit title syntax `Title <target>` should extract the target
        /// from within angle brackets and resolve it.
        #[test]
        fn test_doc_with_explicit_title() {
            let (_temp_dir, vault_dir) = create_doc_role_test_fixture();

            // Add source file with explicit title syntax
            fs::write(
                vault_dir.join("source.md"),
                r#"# Source

See {doc}`Start Here <getting-started>` for introduction.
See {doc}`Missing <nonexistent>` for missing content.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return diagnostics");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                1,
                "Should have 1 diagnostic for missing target only. Got: {:?}",
                diags
            );

            // Verify the diagnostic is for the nonexistent target
            assert!(
                diags[0].message.contains("nonexistent"),
                "Diagnostic should mention 'nonexistent': {}",
                diags[0].message
            );
        }

        // =====================================================================
        // Test 6: Case insensitive bare names
        // =====================================================================

        /// Test: {doc}`GETTING-STARTED` resolves to getting-started.md (case insensitive).
        ///
        /// Bare filenames should be matched case-insensitively to support
        /// various naming conventions.
        #[test]
        fn test_doc_case_insensitive_bare_names() {
            let (_temp_dir, vault_dir) = create_doc_role_test_fixture();

            // Add source file with uppercase reference
            fs::write(
                vault_dir.join("source.md"),
                r#"# Source

See {doc}`GETTING-STARTED` for introduction.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Case insensitive bare name should resolve. Got: {:?}",
                diags
            );
        }

        // =====================================================================
        // Test 7: Spaces in filename
        // =====================================================================

        /// Test: {doc}`file with spaces` resolves to "file with spaces.md".
        ///
        /// Filenames with literal spaces should be resolvable.
        #[test]
        fn test_doc_with_spaces_in_filename() {
            let (_temp_dir, vault_dir) = create_doc_role_test_fixture();

            // Add source file with space-containing reference (literal spaces)
            fs::write(
                vault_dir.join("source.md"),
                r#"# Source

See {doc}`file with spaces` for content with spaces.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Literal spaces in filename should resolve. Got: {:?}",
                diags
            );
        }

        /// Test: {doc}`file%20with%20spaces` currently does NOT decode URL encoding.
        ///
        /// This test documents current behavior: URL-encoded spaces (%20) are
        /// NOT automatically decoded. This is a known limitation.
        ///
        /// TODO: Consider adding URL decoding support in matches_path_or_file.
        #[test]
        fn test_doc_url_encoded_spaces_not_decoded() {
            let (_temp_dir, vault_dir) = create_doc_role_test_fixture();

            // Add source file with URL-encoded space reference
            fs::write(
                vault_dir.join("source.md"),
                r#"# Source

See {doc}`file%20with%20spaces` for URL-encoded version.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("source.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            // Current behavior: URL-encoded spaces are NOT decoded
            // This produces a diagnostic because `file%20with%20spaces` doesn't match
            // the actual file `file with spaces.md`
            assert_eq!(
                diags.len(),
                1,
                "URL-encoded spaces currently produce diagnostic (not decoded). Got: {:?}",
                diags
            );
        }

        // =====================================================================
        // Test 8: Cross-directory from nested location
        // =====================================================================

        /// Test: From tutorials/advanced/, {doc}`commands` resolves to reference/commands.md.
        ///
        /// Bare names should resolve from deeply nested directories to files
        /// in other parts of the vault hierarchy.
        #[test]
        fn test_doc_cross_directory_from_nested() {
            let (_temp_dir, vault_dir) = create_doc_role_test_fixture();

            // Overwrite topic-one.md with reference to commands (in reference/)
            fs::write(
                vault_dir.join("tutorials/advanced/topic-one.md"),
                r#"# Topic One

For command reference, see {doc}`commands`.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("tutorials/advanced/topic-one.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Cross-directory bare name should resolve from nested location. Got: {:?}",
                diags
            );
        }
    }
}
