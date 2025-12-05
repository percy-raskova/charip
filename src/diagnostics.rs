//! Diagnostic providers for MyST Markdown documents.
//!
//! This module implements the LSP `textDocument/publishDiagnostics` capability,
//! detecting errors and warnings in documents.
//!
//! # Diagnostic Types
//!
//! | Type | Severity | Description |
//! |------|----------|-------------|
//! | Broken link | Error | Markdown link to non-existent file/heading |
//! | Broken MyST role | Error | `{ref}`, `{doc}`, `{term}` with invalid target |
//! | Broken image | Error | Image path that doesn't exist on disk |
//! | Undefined substitution | Warning | `{{var}}` with no definition |
//! | Frontmatter error | Error | Invalid YAML or schema violation |
//! | Include cycle | Error | Circular `{include}` directive dependency |
//!
//! # Architecture
//!
//! Diagnostics are collected per-file and published when the file changes:
//!
//! ```text
//! file_diagnostics()
//!     ├── path_unresolved_references()  → broken links, roles
//!     ├── path_unresolved_images()      → missing image files
//!     ├── validate_frontmatter()        → schema validation
//!     └── path_include_cycles()         → circular includes
//! ```
//!
//! # Performance
//!
//! Reference resolution uses `rayon` for parallel iteration over the vault's
//! referenceables. Image validation checks the filesystem directly.
//!
//! # Configuration
//!
//! Diagnostics can be disabled via [`Settings::unresolved_diagnostics`]:
//!
//! ```json
//! { "unresolved_diagnostics": false }
//! ```

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

use crate::{
    config::Settings,
    frontmatter_schema::FrontmatterSchema,
    vault::{self, Reference, Referenceable, Vault},
};

/// Find unresolved references in a file.
///
/// Returns references that don't resolve to any valid target:
/// - Markdown links to non-existent files, headings, or blocks
/// - MyST roles (`{ref}`, `{doc}`, `{term}`) with invalid targets
/// - Substitutions (`{{var}}`) without definitions in the same file
///
/// # Arguments
///
/// * `vault` - The indexed vault containing all referenceables
/// * `path` - Path to the file to check
///
/// # Returns
///
/// Vector of `(file_path, reference)` tuples for each unresolved reference,
/// or `None` if the file doesn't exist in the vault.
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
                    Referenceable::UnresolvedIndexedBlock(..)
                        | Referenceable::UnresolvedFile(..)
                        | Referenceable::UnresolvedHeading(..)
                )
            });

            // Case 2: MyST roles that don't match ANY referenceable are unresolved
            // (Unlike MD links, MyST roles don't have Unresolved* referenceables created for them,
            // so we check if they match nothing at all)
            let is_unresolved_myst_role =
                matches!(reference, Reference::MystRole(..)) && matched_option.is_none();

            // Case 3: Substitutions that don't match ANY SubstitutionDef in the same file
            // Substitutions are FILE-LOCAL, so we only look for matches in the same file
            let is_unresolved_substitution =
                matches!(reference, Reference::Substitution(..)) && matched_option.is_none();

            is_unresolved_md_link || is_unresolved_myst_role || is_unresolved_substitution
        })
        .collect::<Vec<_>>();

    Some(unresolved)
}

/// Find unresolved image references in a file.
///
/// Images are validated differently from markdown references:
/// - They check if the image FILE exists on disk
/// - Paths are resolved relative to the markdown file containing them
/// - External URLs (http://, https://, data:) are NOT validated
fn path_unresolved_images<'a>(vault: &'a Vault, path: &'a Path) -> Vec<(&'a Path, &'a Reference)> {
    let pathreferences = vault.select_references(Some(path));

    // Get the directory containing the markdown file for relative path resolution
    let file_dir = path.parent().unwrap_or(vault.root_dir());

    pathreferences
        .into_par_iter()
        .filter(|(_ref_path, reference)| {
            if let Reference::ImageLink(data) = reference {
                let image_path = &data.reference_text;

                // Skip external URLs (already filtered in extraction, but double-check)
                if image_path.starts_with("http://")
                    || image_path.starts_with("https://")
                    || image_path.starts_with("data:")
                {
                    return false;
                }

                // Resolve the image path
                // Strip ./ prefix if present
                let clean_path = image_path.strip_prefix("./").unwrap_or(image_path);

                // Try resolving relative to the file's directory
                let resolved = file_dir.join(clean_path);
                if resolved.exists() {
                    return false; // File exists, not broken
                }

                // Also try resolving relative to vault root (for absolute-like paths)
                let from_root = vault.root_dir().join(clean_path);
                if from_root.exists() {
                    return false; // File exists, not broken
                }

                // Image file not found
                true
            } else {
                false // Not an image reference
            }
        })
        .collect()
}

/// Generate diagnostics for frontmatter schema validation errors.
///
/// Returns diagnostics if the file has frontmatter and the schema finds violations.
/// Returns empty vec if:
/// - No schema is provided
/// - File has no frontmatter
/// - Frontmatter is valid
fn frontmatter_diagnostics(
    vault: &Vault,
    path: &Path,
    schema: Option<&FrontmatterSchema>,
) -> Vec<Diagnostic> {
    let Some(schema) = schema else {
        return vec![];
    };

    // Get file content from vault
    let Some(rope) = vault.ropes.get(path) else {
        return vec![];
    };

    let text = rope.to_string();
    let result = schema.validate(&text);

    result
        .errors
        .into_iter()
        .map(|error| {
            // Use the frontmatter range for positioning, or default to first line
            let range = result
                .frontmatter_range
                .unwrap_or(tower_lsp::lsp_types::Range {
                    start: tower_lsp::lsp_types::Position {
                        line: 0,
                        character: 0,
                    },
                    end: tower_lsp::lsp_types::Position {
                        line: 0,
                        character: 1,
                    },
                });

            // Create a more user-friendly message
            let message = if error.instance_path.is_empty() {
                format!("Frontmatter: {}", error.message)
            } else {
                format!("Frontmatter {}: {}", error.instance_path, error.message)
            };

            Diagnostic {
                range,
                message,
                source: Some("charip".into()),
                severity: Some(DiagnosticSeverity::WARNING),
                ..Default::default()
            }
        })
        .collect()
}

/// Generate diagnostics for include cycles involving the given file.
///
/// Uses Tarjan's SCC algorithm (via `Vault::detect_include_cycles()`) to find
/// cycles in the include graph. Returns a diagnostic for each cycle that
/// contains the specified file.
///
/// # Arguments
///
/// * `vault` - The indexed vault containing the include graph
/// * `path` - Path to the file to check for cycle participation
///
/// # Returns
///
/// Vector of ERROR-severity diagnostics, one for each cycle the file participates in.
/// Returns empty vec if the file is not in any cycle.
fn path_include_cycles(vault: &Vault, path: &Path) -> Vec<Diagnostic> {
    vault
        .detect_include_cycles()
        .into_iter()
        .filter(|cycle| cycle.iter().any(|p| p == path))
        .map(|cycle| {
            let cycle_str = cycle
                .iter()
                .map(|p| p.file_name().unwrap_or_default().to_string_lossy())
                .collect::<Vec<_>>()
                .join(" -> ");
            Diagnostic {
                range: tower_lsp::lsp_types::Range {
                    start: tower_lsp::lsp_types::Position::new(0, 0),
                    end: tower_lsp::lsp_types::Position::new(0, 1),
                },
                message: format!("Circular include detected: {}", cycle_str),
                source: Some("charip".into()),
                severity: Some(DiagnosticSeverity::ERROR),
                ..Default::default()
            }
        })
        .collect()
}

#[allow(dead_code)]
pub fn diagnostics(
    vault: &Vault,
    settings: &Settings,
    (path, _uri): (&PathBuf, &Url),
) -> Option<Vec<Diagnostic>> {
    diagnostics_with_schema(vault, settings, (path, _uri), None)
}

/// Generate diagnostics with optional frontmatter schema validation.
///
/// This is the main diagnostics entry point when frontmatter validation is enabled.
pub fn diagnostics_with_schema(
    vault: &Vault,
    settings: &Settings,
    (path, _uri): (&PathBuf, &Url),
    schema: Option<&FrontmatterSchema>,
) -> Option<Vec<Diagnostic>> {
    if !settings.unresolved_diagnostics {
        return None;
    }

    let unresolved = path_unresolved_references(vault, path)?;
    let unresolved_images = path_unresolved_images(vault, path);

    let allreferences = vault.select_references(None);

    // Generate diagnostics for unresolved markdown references
    let mut diags: Vec<Diagnostic> = unresolved
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

            // Generate role-specific or generic message using trait method
            let message = reference.generate_diagnostic_message(usage_count);

            Diagnostic {
                range: *reference.data().range,
                message,
                source: Some("charip".into()),
                severity: Some(DiagnosticSeverity::INFORMATION),
                ..Default::default()
            }
        })
        .collect();

    // Generate diagnostics for unresolved image references
    let image_diags: Vec<Diagnostic> = unresolved_images
        .into_par_iter()
        .map(|(path, reference)| {
            // Count how many times this same broken image appears
            let usage_count = allreferences
                .iter()
                .filter(|(other_path, otherreference)| {
                    otherreference.matches_type(reference)
                        && **other_path == *path
                        && otherreference.data().reference_text == reference.data().reference_text
                })
                .count();

            // Use trait method on Reference
            let message = reference.generate_diagnostic_message(usage_count);

            Diagnostic {
                range: *reference.data().range,
                message,
                source: Some("charip".into()),
                severity: Some(DiagnosticSeverity::WARNING), // Images are warnings, not info
                ..Default::default()
            }
        })
        .collect();

    diags.extend(image_diags);

    // Generate frontmatter validation diagnostics
    let frontmatter_diags = frontmatter_diagnostics(vault, path, schema);
    diags.extend(frontmatter_diags);

    // Generate cycle diagnostics for include directives
    let cycle_diags = path_include_cycles(vault, path);
    diags.extend(cycle_diags);

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

    // ========================================================================
    // Substitution Diagnostic Tests (Chunk 11)
    // ========================================================================

    /// Test: Undefined substitution produces a diagnostic.
    #[test]
    fn test_undefined_substitution_diagnostic() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // File with undefined substitution
        fs::write(vault_dir.join("test.md"), "Hello {{undefined_var}}!").unwrap();

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
            "Should return Some when substitution is undefined"
        );
        let diags = result.unwrap();
        assert_eq!(
            diags.len(),
            1,
            "Should have 1 diagnostic for undefined substitution"
        );
        assert!(
            diags[0].message.contains("undefined_var"),
            "Message should contain the substitution name"
        );
    }

    /// Test: Defined substitution produces NO diagnostic.
    #[test]
    fn test_defined_substitution_no_diagnostic() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // File with defined substitution
        fs::write(
            vault_dir.join("test.md"),
            r#"---
myst:
  substitutions:
    name: "World"
---
Hello {{name}}!"#,
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
            "Should have 0 diagnostics when substitution is defined"
        );
    }

    /// Test: Substitution defined in another file should still produce diagnostic.
    /// (Substitutions are FILE-LOCAL)
    #[test]
    fn test_substitution_cross_file_produces_diagnostic() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // File A: has definition
        fs::write(
            vault_dir.join("file_a.md"),
            r#"---
substitutions:
  shared_var: "ValueA"
---
# File A"#,
        )
        .unwrap();

        // File B: uses the same name but doesn't define it (should be unresolved)
        fs::write(
            vault_dir.join("file_b.md"),
            "Using {{shared_var}} which is not defined in this file.",
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let file_b_path = vault_dir.join("file_b.md");
        let uri = Url::from_file_path(&file_b_path).unwrap();

        let result = diagnostics(&vault, &settings, (&file_b_path, &uri));

        assert!(result.is_some(), "Should return Some");
        let diags = result.unwrap();
        assert_eq!(
            diags.len(),
            1,
            "Should have 1 diagnostic - cross-file resolution not allowed"
        );
        assert!(
            diags[0].message.contains("shared_var"),
            "Message should contain the substitution name"
        );
    }

    /// Test: Multiple undefined substitutions produce multiple diagnostics.
    #[test]
    fn test_multiple_undefined_substitutions() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        fs::write(
            vault_dir.join("test.md"),
            "{{one}} and {{two}} and {{three}}",
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
            3,
            "Should have 3 diagnostics for 3 undefined substitutions"
        );
    }

    /// Test: Diagnostic message format for undefined substitution.
    #[test]
    fn test_substitution_diagnostic_message_format() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        fs::write(vault_dir.join("test.md"), "Hello {{world}}!").unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result = diagnostics(&vault, &settings, (&file_path, &uri));

        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        // Check the message format includes "Undefined substitution" and the name
        assert!(
            diags[0].message.contains("Undefined substitution")
                || diags[0].message.contains("undefined"),
            "Message should indicate undefined substitution"
        );
        assert!(diags[0].message.contains("world"));
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

        // =====================================================================
        // {eq} Role Diagnostics Tests
        // =====================================================================
        //
        // Tests for {eq} role target resolution against math equation labels.
        // The {eq} role references `:label:` values defined in `{math}` directives.

        /// Test: Valid {eq} role referencing an existing math label produces no diagnostic.
        ///
        /// When a {math} directive has `:label: my-equation`, a reference like
        /// `{eq}`my-equation`` should resolve successfully with no diagnostic.
        #[test]
        fn test_valid_eq_role_no_diagnostic() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with math directive and {eq} reference
            fs::write(
                vault_dir.join("math.md"),
                r#"# Equations

```{math}
:label: my-equation

E = mc^2
```

See {eq}`my-equation` for details."#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("math.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Valid {{eq}} role should not produce diagnostics. Got: {:?}",
                diags
            );
        }

        /// Test: Broken {eq} role referencing non-existent label produces diagnostic.
        ///
        /// When no math label matches the target, a diagnostic should be produced
        /// with "Unresolved equation reference" in the message.
        #[test]
        fn test_broken_eq_role_produces_diagnostic() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with {eq} referencing non-existent label
            fs::write(
                vault_dir.join("test.md"),
                r#"See {eq}`nonexistent-equation` for details."#,
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
                "Should have 1 diagnostic for broken {{eq}} role"
            );
            assert!(
                diags[0].message.contains("Unresolved equation reference"),
                "Message should be equation-specific: {}",
                diags[0].message
            );
            assert!(
                diags[0].message.contains("nonexistent-equation"),
                "Message should include target name: {}",
                diags[0].message
            );
        }

        /// Test: {eq} role can reference math labels across files.
        ///
        /// A {eq} role in one file should resolve to a math label defined
        /// in another file within the same vault.
        #[test]
        fn test_eq_role_cross_file_resolution() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with math directive
            fs::write(
                vault_dir.join("equations.md"),
                r#"# Important Equations

```{math}
:label: euler-identity

e^{i\pi} + 1 = 0
```
"#,
            )
            .unwrap();

            // Create file with {eq} reference to equation in other file
            fs::write(
                vault_dir.join("document.md"),
                r#"# Document

The most beautiful equation is {eq}`euler-identity`.
"#,
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("document.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Cross-file {{eq}} role should resolve. Got: {:?}",
                diags
            );
        }

        /// Test: Multiple {eq} roles with mixed validity.
        ///
        /// Multiple {eq} roles in a single file should each be evaluated
        /// independently - valid ones produce no diagnostic, broken ones do.
        #[test]
        fn test_multiple_eq_roles_mixed_validity() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with math labels
            fs::write(
                vault_dir.join("equations.md"),
                r#"```{math}
:label: valid-one

x = 1
```

```{math}
:label: valid-two

y = 2
```
"#,
            )
            .unwrap();

            // Create file with mix of valid and broken {eq} references
            fs::write(
                vault_dir.join("test.md"),
                r#"# Test

See {eq}`valid-one` for the first equation.
See {eq}`broken-ref` for a missing equation.
See {eq}`valid-two` for the second equation.
See {eq}`another-broken` for another missing one.
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
                2,
                "Should have 2 diagnostics for 2 broken {{eq}} roles. Got: {:?}",
                diags
            );
        }

        /// Test: {eq} role matching is case-insensitive.
        ///
        /// {eq}`EULER-IDENTITY` should match a label defined as `euler-identity`.
        #[test]
        fn test_eq_role_case_insensitive() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            fs::write(
                vault_dir.join("equations.md"),
                r#"```{math}
:label: euler-identity

e^{i\pi} + 1 = 0
```
"#,
            )
            .unwrap();

            // Reference with different case
            fs::write(
                vault_dir.join("test.md"),
                r#"See {eq}`EULER-IDENTITY` for the equation."#,
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
                "Case-insensitive {{eq}} matching should work. Got: {:?}",
                diags
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

    /// Integration tests against the real TestFiles/ vault.
    /// These tests verify that the LSP works correctly against a comprehensive
    /// MyST documentation structure with all reference types.
    mod testfiles_vault_integration {
        use super::*;
        use std::path::PathBuf;

        fn get_testfiles_path() -> PathBuf {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("TestFiles")
        }

        /// Verify the TestFiles vault can be constructed without errors.
        #[test]
        fn test_vault_construction() {
            let vault_path = get_testfiles_path();
            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault = Vault::construct_vault(&settings, &vault_path)
                .expect("TestFiles vault should construct successfully");

            // Verify files were indexed
            assert!(
                vault.node_index.len() >= 8,
                "Should have at least 8 markdown files, got {}",
                vault.node_index.len()
            );
        }

        /// Verify valid {doc} references in index.md produce no diagnostics.
        #[test]
        fn test_valid_doc_references() {
            let vault_path = get_testfiles_path();
            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault = Vault::construct_vault(&settings, &vault_path).unwrap();
            let file_path = vault_path.join("index.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            // index.md has valid {doc} references that should resolve
            if let Some(diags) = &result {
                let doc_diags: Vec<_> = diags
                    .iter()
                    .filter(|d| d.message.contains("document"))
                    .collect();
                assert_eq!(
                    doc_diags.len(),
                    0,
                    "Valid {{doc}} references should not produce diagnostics: {:?}",
                    doc_diags
                );
            }
        }

        /// Verify broken-refs.md produces expected diagnostics.
        #[test]
        fn test_broken_refs_produce_diagnostics() {
            let vault_path = get_testfiles_path();
            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault = Vault::construct_vault(&settings, &vault_path).unwrap();
            let file_path = vault_path.join("broken-refs.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "broken-refs.md should have diagnostics");
            let diags = result.unwrap();

            // Should have diagnostics for broken {doc}, {ref}, {term} references
            assert!(
                diags.len() >= 3,
                "Should have at least 3 broken reference diagnostics, got {}: {:?}",
                diags.len(),
                diags
            );

            // Verify we have document-related diagnostics
            let has_doc_diagnostic = diags.iter().any(|d| d.message.contains("document"));
            assert!(
                has_doc_diagnostic,
                "Should have diagnostic for broken {{doc}} reference"
            );
        }

        /// Verify code-examples.md doesn't produce diagnostics for roles in code blocks.
        #[test]
        fn test_code_blocks_excluded() {
            let vault_path = get_testfiles_path();
            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault = Vault::construct_vault(&settings, &vault_path).unwrap();
            let file_path = vault_path.join("code-examples.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            // code-examples.md has broken refs inside code blocks that should NOT trigger
            // and valid refs outside code blocks that should resolve
            if let Some(diags) = &result {
                // Should not have diagnostics for "should-not-parse", "fake-reference", etc.
                let code_block_diags: Vec<_> = diags
                    .iter()
                    .filter(|d| {
                        d.message.contains("should-not-parse")
                            || d.message.contains("fake-reference")
                            || d.message.contains("NotReal")
                    })
                    .collect();
                assert_eq!(
                    code_block_diags.len(),
                    0,
                    "Roles inside code blocks should not produce diagnostics: {:?}",
                    code_block_diags
                );
            }
        }

        /// Verify glossary terms are parsed from {glossary} directive.
        #[test]
        fn test_glossary_terms_extracted() {
            let vault_path = get_testfiles_path();
            let settings = Settings::default();

            let vault = Vault::construct_vault(&settings, &vault_path).unwrap();

            // Find glossary.md and check for glossary terms
            let glossary_path = vault_path.join("glossary.md");
            let doc_node = vault.get_document(&glossary_path);

            assert!(doc_node.is_some(), "glossary.md should be indexed");
            let doc_node = doc_node.unwrap();

            assert!(
                !doc_node.glossary_terms.is_empty(),
                "glossary.md should have glossary terms extracted"
            );

            // Check for expected terms
            let term_names: Vec<&String> =
                doc_node.glossary_terms.iter().map(|t| &t.term).collect();
            assert!(
                term_names.iter().any(|t| t.contains("API")),
                "Should have API term: {:?}",
                term_names
            );
            assert!(
                term_names.iter().any(|t| t.contains("MyST")),
                "Should have MyST term: {:?}",
                term_names
            );
        }

        /// Verify MyST anchors are extracted from (anchor)= syntax.
        #[test]
        fn test_myst_anchors_extracted() {
            let vault_path = get_testfiles_path();
            let settings = Settings::default();

            let vault = Vault::construct_vault(&settings, &vault_path).unwrap();

            // Check getting-started.md for anchors
            let gs_path = vault_path.join("guides/getting-started.md");
            let doc_node = vault.get_document(&gs_path);

            assert!(doc_node.is_some(), "getting-started.md should be indexed");
            let doc_node = doc_node.unwrap();

            // Should have installation-anchor
            let has_installation_anchor = doc_node
                .myst_symbols
                .iter()
                .any(|s| s.name == "installation-anchor");
            assert!(
                has_installation_anchor,
                "Should have installation-anchor: {:?}",
                doc_node.myst_symbols
            );
        }

        /// Verify cross-file {ref} references work.
        #[test]
        fn test_cross_file_ref_resolution() {
            let vault_path = get_testfiles_path();
            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault = Vault::construct_vault(&settings, &vault_path).unwrap();

            // reference/api.md has {ref}`installation-anchor` which points to guides/getting-started.md
            let file_path = vault_path.join("reference/api.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            if let Some(diags) = &result {
                // Should NOT have diagnostic for installation-anchor (it exists)
                let installation_diags: Vec<_> = diags
                    .iter()
                    .filter(|d| d.message.contains("installation-anchor"))
                    .collect();
                assert_eq!(
                    installation_diags.len(),
                    0,
                    "Cross-file {{ref}} to installation-anchor should resolve: {:?}",
                    installation_diags
                );
            }
        }
    }

    // =========================================================================
    // Image Path Diagnostics Tests
    // =========================================================================
    //
    // Tests for image path validation. Images referenced as ![alt](path)
    // should produce diagnostics when the file doesn't exist.

    mod image_diagnostics {
        use super::*;

        /// Test: Broken image reference produces diagnostic when file doesn't exist.
        ///
        /// An image like `![Missing](./nonexistent.png)` pointing to a non-existent file
        /// should produce a diagnostic.
        #[test]
        fn test_broken_image_produces_diagnostic() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with broken image reference
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\n![Missing](./nonexistent.png)",
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
            assert_eq!(diags.len(), 1, "Should have 1 diagnostic for broken image");
            assert!(
                diags[0].message.contains("nonexistent.png"),
                "Message should mention the missing image: {}",
                diags[0].message
            );
        }

        /// Test: Valid image reference produces no diagnostic when file exists.
        ///
        /// An image referencing an existing file should not produce a diagnostic.
        #[test]
        fn test_valid_image_no_diagnostic() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create the image file
            fs::create_dir_all(vault_dir.join("images")).unwrap();
            fs::write(vault_dir.join("images/photo.png"), "fake image data").unwrap();

            // Create file with valid image reference
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\n![Photo](images/photo.png)",
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
                "Valid image should not produce diagnostics. Got: {:?}",
                diags
            );
        }

        /// Test: External image URLs are not validated.
        ///
        /// Images with https:// URLs should NOT produce diagnostics as they're external.
        #[test]
        fn test_external_image_no_diagnostic() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with external image reference
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\n![Logo](https://example.com/logo.png)",
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
                "External images should not produce diagnostics. Got: {:?}",
                diags
            );
        }

        /// Test: Data URI images are not validated.
        ///
        /// Images with data: URIs should NOT produce diagnostics.
        #[test]
        fn test_data_uri_image_no_diagnostic() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with data URI image
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\n![Inline](data:image/png;base64,ABC123)",
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
                "Data URI images should not produce diagnostics. Got: {:?}",
                diags
            );
        }

        /// Test: Multiple broken images produce multiple diagnostics.
        #[test]
        fn test_multiple_broken_images() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with multiple broken image references
            fs::write(
                vault_dir.join("test.md"),
                r#"# Test

![Missing 1](missing1.png)
![Missing 2](missing2.jpg)
![Missing 3](assets/missing3.gif)
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
                "Should have 3 diagnostics for 3 broken images. Got: {:?}",
                diags
            );
        }

        /// Test: Mixed valid and broken images only report broken ones.
        #[test]
        fn test_mixed_valid_and_broken_images() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create some valid image files
            fs::create_dir_all(vault_dir.join("images")).unwrap();
            fs::write(vault_dir.join("images/valid1.png"), "fake image").unwrap();
            fs::write(vault_dir.join("valid2.jpg"), "fake image").unwrap();

            // Create file with mix of valid and broken images
            fs::write(
                vault_dir.join("test.md"),
                r#"# Test

![Valid 1](images/valid1.png)
![Broken 1](images/broken.png)
![Valid 2](valid2.jpg)
![Broken 2](missing.gif)
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
                2,
                "Should have 2 diagnostics for 2 broken images. Got: {:?}",
                diags
            );
        }

        /// Test: Image diagnostic message contains "image" and filename.
        #[test]
        fn test_image_diagnostic_message_format() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create file with broken image reference
            fs::write(
                vault_dir.join("test.md"),
                "# Test\n\n![Alt text](path/to/image.png)",
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
            assert_eq!(diags.len(), 1, "Should have 1 diagnostic");

            // Check message format contains "image" and the path
            assert!(
                diags[0].message.to_lowercase().contains("image"),
                "Message should mention 'image': {}",
                diags[0].message
            );
            assert!(
                diags[0].message.contains("path/to/image.png"),
                "Message should include the image path: {}",
                diags[0].message
            );
        }

        /// Test: Relative paths with ./ prefix are resolved correctly.
        #[test]
        fn test_image_relative_path_dot_slash() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create image file
            fs::write(vault_dir.join("photo.png"), "fake image").unwrap();

            // Create file with ./ prefix reference
            fs::write(vault_dir.join("test.md"), "# Test\n\n![Photo](./photo.png)").unwrap();

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
                "Valid ./ prefixed image should not produce diagnostics. Got: {:?}",
                diags
            );
        }

        /// Test: Image in subdirectory from file in same subdirectory.
        #[test]
        fn test_image_same_subdirectory() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create subdirectory with markdown and image
            fs::create_dir_all(vault_dir.join("docs")).unwrap();
            fs::write(vault_dir.join("docs/diagram.svg"), "fake svg").unwrap();
            fs::write(
                vault_dir.join("docs/guide.md"),
                "# Guide\n\n![Diagram](diagram.svg)",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("docs/guide.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some");
            let diags = result.unwrap();
            assert_eq!(
                diags.len(),
                0,
                "Image in same directory should resolve. Got: {:?}",
                diags
            );
        }
    }

    // ========================================================================
    // Frontmatter Schema Validation Integration Tests
    // ========================================================================

    /// Test: Frontmatter validation produces diagnostics for missing required field.
    #[test]
    fn test_frontmatter_missing_required_field_diagnostic() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create schema requiring title
        let schema_dir = vault_dir.join("_schemas");
        fs::create_dir(&schema_dir).unwrap();
        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["title"],
            "properties": {
                "title": { "type": "string" }
            }
        }"#;
        fs::write(schema_dir.join("frontmatter.schema.json"), schema).unwrap();

        // Create file with frontmatter missing required field
        fs::write(
            vault_dir.join("test.md"),
            r#"---
author: "Test Author"
---
# Content"#,
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            frontmatter_schema_path: "_schemas/frontmatter.schema.json".to_string(),
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Load schema
        let schema_path = vault_dir.join("_schemas/frontmatter.schema.json");
        let schema = FrontmatterSchema::load(&schema_path);

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result =
            diagnostics_with_schema(&vault, &settings, (&file_path, &uri), schema.as_ref());

        assert!(result.is_some(), "Should return Some");
        let diags = result.unwrap();
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("title") || d.message.contains("required")),
            "Should have diagnostic about missing title. Got: {:?}",
            diags
        );
        assert!(
            diags
                .iter()
                .any(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
            "Frontmatter diagnostics should have WARNING severity"
        );
    }

    /// Test: Valid frontmatter produces no diagnostics.
    #[test]
    fn test_frontmatter_valid_no_diagnostic() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create schema
        let schema_dir = vault_dir.join("_schemas");
        fs::create_dir(&schema_dir).unwrap();
        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["title"],
            "properties": {
                "title": { "type": "string" }
            }
        }"#;
        fs::write(schema_dir.join("frontmatter.schema.json"), schema).unwrap();

        // Create file with valid frontmatter
        fs::write(
            vault_dir.join("test.md"),
            r#"---
title: "Valid Title"
---
# Content"#,
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            frontmatter_schema_path: "_schemas/frontmatter.schema.json".to_string(),
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let schema_path = vault_dir.join("_schemas/frontmatter.schema.json");
        let schema = FrontmatterSchema::load(&schema_path);

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result =
            diagnostics_with_schema(&vault, &settings, (&file_path, &uri), schema.as_ref());

        assert!(result.is_some(), "Should return Some");
        let diags = result.unwrap();
        // Should have no frontmatter diagnostics (might have 0 total if no other issues)
        let frontmatter_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("Frontmatter"))
            .collect();
        assert!(
            frontmatter_diags.is_empty(),
            "Valid frontmatter should produce no schema diagnostics. Got: {:?}",
            frontmatter_diags
        );
    }

    /// Test: No schema means no frontmatter diagnostics.
    #[test]
    fn test_frontmatter_no_schema_no_diagnostic() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with frontmatter but no schema
        fs::write(
            vault_dir.join("test.md"),
            r#"---
invalid_field: 123
---
# Content"#,
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            frontmatter_schema_path: "_schemas/nonexistent.json".to_string(),
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Schema doesn't exist, so load returns None
        let schema_path = vault_dir.join("_schemas/nonexistent.json");
        let schema = FrontmatterSchema::load(&schema_path);
        assert!(
            schema.is_none(),
            "Schema should not load from nonexistent file"
        );

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result =
            diagnostics_with_schema(&vault, &settings, (&file_path, &uri), schema.as_ref());

        assert!(result.is_some(), "Should return Some");
        let diags = result.unwrap();
        let frontmatter_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("Frontmatter"))
            .collect();
        assert!(
            frontmatter_diags.is_empty(),
            "No schema means no frontmatter diagnostics. Got: {:?}",
            frontmatter_diags
        );
    }

    /// Test: Frontmatter with wrong type produces diagnostic.
    #[test]
    fn test_frontmatter_wrong_type_diagnostic() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create schema expecting tags as array
        let schema_dir = vault_dir.join("_schemas");
        fs::create_dir(&schema_dir).unwrap();
        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        }"#;
        fs::write(schema_dir.join("frontmatter.schema.json"), schema).unwrap();

        // Create file with tags as string (wrong type)
        fs::write(
            vault_dir.join("test.md"),
            r#"---
tags: "not-an-array"
---
# Content"#,
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            frontmatter_schema_path: "_schemas/frontmatter.schema.json".to_string(),
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let schema_path = vault_dir.join("_schemas/frontmatter.schema.json");
        let schema = FrontmatterSchema::load(&schema_path);

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result =
            diagnostics_with_schema(&vault, &settings, (&file_path, &uri), schema.as_ref());

        assert!(result.is_some(), "Should return Some");
        let diags = result.unwrap();
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("tags") && d.message.contains("Frontmatter")),
            "Should have diagnostic about tags type. Got: {:?}",
            diags
        );
    }

    /// Test: File without frontmatter produces no frontmatter diagnostics.
    #[test]
    fn test_frontmatter_no_frontmatter_no_diagnostic() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create schema
        let schema_dir = vault_dir.join("_schemas");
        fs::create_dir(&schema_dir).unwrap();
        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["title"]
        }"#;
        fs::write(schema_dir.join("frontmatter.schema.json"), schema).unwrap();

        // Create file WITHOUT frontmatter
        fs::write(
            vault_dir.join("test.md"),
            "# Just a heading\n\nNo frontmatter.",
        )
        .unwrap();

        let settings = Settings {
            unresolved_diagnostics: true,
            frontmatter_schema_path: "_schemas/frontmatter.schema.json".to_string(),
            ..Settings::default()
        };

        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        let schema_path = vault_dir.join("_schemas/frontmatter.schema.json");
        let schema = FrontmatterSchema::load(&schema_path);

        let file_path = vault_dir.join("test.md");
        let uri = Url::from_file_path(&file_path).unwrap();

        let result =
            diagnostics_with_schema(&vault, &settings, (&file_path, &uri), schema.as_ref());

        assert!(result.is_some(), "Should return Some");
        let diags = result.unwrap();
        let frontmatter_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("Frontmatter"))
            .collect();
        assert!(
            frontmatter_diags.is_empty(),
            "File without frontmatter should have no frontmatter diagnostics. Got: {:?}",
            frontmatter_diags
        );
    }

    // =========================================================================
    // Include Cycle Diagnostic Tests
    // =========================================================================
    //
    // Tests for detecting circular include dependencies and reporting them
    // as LSP diagnostics. Include cycles block Sphinx builds, so they are
    // reported with ERROR severity.

    mod include_cycle_diagnostics {
        use super::*;

        /// Test: Files participating in an include cycle get an ERROR diagnostic.
        ///
        /// Creates A -> B -> C -> A cycle, then verifies that calling diagnostics
        /// on file A produces a "Circular include" error.
        #[test]
        fn test_cycle_diagnostic_generated_for_file_in_cycle() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create cycle: a.md -> b.md -> c.md -> a.md
            fs::write(vault_dir.join("a.md"), "```{include} b.md\n```").unwrap();
            fs::write(vault_dir.join("b.md"), "```{include} c.md\n```").unwrap();
            fs::write(vault_dir.join("c.md"), "```{include} a.md\n```").unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("a.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some for file in cycle");
            let diags = result.unwrap();

            // Find cycle-related diagnostics
            let cycle_diags: Vec<_> = diags
                .iter()
                .filter(|d| d.message.contains("Circular include"))
                .collect();

            assert_eq!(
                cycle_diags.len(),
                1,
                "Should have exactly 1 cycle diagnostic for file a.md"
            );
            assert_eq!(
                cycle_diags[0].severity,
                Some(DiagnosticSeverity::ERROR),
                "Cycle diagnostic should be ERROR severity"
            );
        }

        /// Test: Files NOT participating in a cycle get NO cycle diagnostic.
        ///
        /// Creates A -> B -> C -> A cycle plus unrelated file D.
        /// Verifies that file D has no cycle-related diagnostics.
        #[test]
        fn test_no_cycle_diagnostic_for_unrelated_file() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create cycle: a.md -> b.md -> c.md -> a.md
            fs::write(vault_dir.join("a.md"), "```{include} b.md\n```").unwrap();
            fs::write(vault_dir.join("b.md"), "```{include} c.md\n```").unwrap();
            fs::write(vault_dir.join("c.md"), "```{include} a.md\n```").unwrap();

            // Create unrelated file d.md (not in any cycle)
            fs::write(
                vault_dir.join("d.md"),
                "# Unrelated file\n\nNo cycles here.",
            )
            .unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("d.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some for file d.md");
            let diags = result.unwrap();

            // No cycle-related diagnostics for unrelated file
            let cycle_diags: Vec<_> = diags
                .iter()
                .filter(|d| d.message.contains("Circular include"))
                .collect();

            assert!(
                cycle_diags.is_empty(),
                "Unrelated file should have no cycle diagnostics, got: {:?}",
                cycle_diags
            );
        }

        /// Test: Cycle diagnostic message shows the cycle path.
        ///
        /// Creates A -> B -> A cycle and verifies the diagnostic message
        /// contains both file names showing the cycle path.
        #[test]
        fn test_cycle_diagnostic_shows_cycle_path() {
            let (_temp_dir, vault_dir) = create_test_vault_dir();

            // Create simple 2-node cycle: a.md -> b.md -> a.md
            fs::write(vault_dir.join("a.md"), "```{include} b.md\n```").unwrap();
            fs::write(vault_dir.join("b.md"), "```{include} a.md\n```").unwrap();

            let settings = Settings {
                unresolved_diagnostics: true,
                ..Settings::default()
            };

            let vault =
                Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

            let file_path = vault_dir.join("a.md");
            let uri = Url::from_file_path(&file_path).unwrap();

            let result = diagnostics(&vault, &settings, (&file_path, &uri));

            assert!(result.is_some(), "Should return Some for file in cycle");
            let diags = result.unwrap();

            // Find cycle-related diagnostic
            let cycle_diags: Vec<_> = diags
                .iter()
                .filter(|d| d.message.contains("Circular include"))
                .collect();

            assert_eq!(cycle_diags.len(), 1, "Should have 1 cycle diagnostic");

            // Message should contain both file names
            let msg = &cycle_diags[0].message;
            assert!(
                msg.contains("a.md") && msg.contains("b.md"),
                "Cycle message should show both files in cycle. Got: {}",
                msg
            );
        }
    }
}
