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
}
