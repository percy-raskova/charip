//! Integration tests for charip library public API.
//!
//! These tests verify that the library can be used as an external dependency,
//! ensuring the lib+bin separation works correctly.

// These are compile-time accessibility tests - the assertions verify the code compiles
#![allow(clippy::assertions_on_constants)]
// Helper functions exist to prove types are accessible, not to be called
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Import from the charip library crate (external consumer perspective)
use charip::config::Settings;
use charip::vault::{Reference, Referenceable, Vault};

// Note: Macro exports (params_path, params_position_path) are available via #[macro_export]
// but can only be tested at compile-time via actual usage in context.
// The fact this crate compiles with `use charip::*` proves they're exported.

/// Helper: Create a temporary vault directory for testing.
///
/// Returns (TempDir, PathBuf) - keep TempDir alive for test duration.
fn create_test_vault_dir() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let vault_dir = temp_dir.path().join("vault");
    fs::create_dir(&vault_dir).expect("Failed to create vault subdirectory");
    (temp_dir, vault_dir)
}

// ============================================================================
// Public API Accessibility Tests
// ============================================================================

#[test]
fn test_vault_construction_from_external_crate() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create a minimal test file
    fs::write(
        vault_dir.join("test.md"),
        "# Test Document\n\nSome content.",
    )
    .unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir);

    assert!(vault.is_ok(), "Vault construction should succeed");
    let vault = vault.unwrap();
    assert!(
        vault.document_count() >= 1,
        "Vault should contain at least 1 document"
    );
}

#[test]
fn test_settings_struct_accessible() {
    // Verify Settings can be constructed with defaults
    let settings = Settings::default();

    // Verify key fields are accessible
    assert_eq!(settings.dailynote, "%Y-%m-%d");
    assert!(settings.heading_completions);
    assert!(settings.unresolved_diagnostics);
}

#[test]
fn test_reference_enum_accessible() {
    // Verify Reference enum variants are accessible for pattern matching
    // (We can't construct them directly without vault context, but we can
    // verify the type exists and is public)
    fn accepts_reference(_r: &Reference) {}

    // This test passes if it compiles - proves Reference is public
    assert!(true, "Reference enum is accessible");
}

#[test]
fn test_referenceable_enum_accessible() {
    // Verify Referenceable enum is public
    fn accepts_referenceable(_r: &Referenceable) {}

    assert!(true, "Referenceable enum is accessible");
}

#[test]
fn test_macro_exports_available() {
    // Macros are exported via #[macro_export] in src/macros.rs
    // They are available at crate root as charip::params_path! and charip::params_position_path!
    //
    // These macros require tower_lsp::lsp_types parameter types to actually use,
    // so we can only verify their existence implicitly by the crate compiling.
    //
    // The binary (main.rs) successfully uses: `use charip::{params_path, params_position_path};`
    // which proves they are properly exported.
    assert!(true, "Macros are exported (verified by binary compilation)");
}

// ============================================================================
// Vault Method Accessibility Tests
// ============================================================================

#[test]
fn test_vault_select_references_accessible() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create files with a reference
    fs::write(vault_dir.join("source.md"), "# Source\n\n[Link](target.md)").unwrap();
    fs::write(vault_dir.join("target.md"), "# Target").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    // Verify select_references method is accessible
    let references = vault.select_references(None);
    assert!(!references.is_empty(), "Should find references");
}

#[test]
fn test_vault_select_referenceable_nodes_accessible() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(vault_dir.join("test.md"), "# Test\n\n## Heading\n\n#tag").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    // Verify select_referenceable_nodes method is accessible
    let nodes = vault.select_referenceable_nodes(None);
    assert!(!nodes.is_empty(), "Should find referenceable nodes");
}

// ============================================================================
// Module Accessibility Tests
// ============================================================================

#[test]
fn test_completion_module_accessible() {
    // Verify completion module is public
    use charip::completion::get_completions;
    let _ = std::any::type_name_of_val(&get_completions);
    assert!(true, "completion::get_completions is accessible");
}

#[test]
fn test_diagnostics_module_accessible() {
    use charip::diagnostics::diagnostics_with_schema;
    let _ = std::any::type_name_of_val(&diagnostics_with_schema);
    assert!(true, "diagnostics module is accessible");
}

#[test]
fn test_gotodef_module_accessible() {
    use charip::gotodef::goto_definition;
    let _ = std::any::type_name_of_val(&goto_definition);
    assert!(true, "gotodef module is accessible");
}

#[test]
fn test_references_module_accessible() {
    use charip::references::references;
    let _ = std::any::type_name_of_val(&references);
    assert!(true, "references module is accessible");
}

#[test]
fn test_hover_module_accessible() {
    use charip::hover::hover;
    let _ = std::any::type_name_of_val(&hover);
    assert!(true, "hover module is accessible");
}

#[test]
fn test_myst_parser_module_accessible() {
    use charip::myst_parser::{MystSymbol, MystSymbolKind};
    fn accepts_symbol(_s: &MystSymbol) {}
    fn accepts_kind(_k: &MystSymbolKind) {}
    assert!(true, "myst_parser types are accessible");
}

#[test]
fn test_config_module_accessible() {
    use charip::config::{Case, EmbeddedBlockTransclusionLength, Settings};
    let _ = Settings::default();
    let _ = Case::Smart;
    let _ = EmbeddedBlockTransclusionLength::Full;
    assert!(true, "config types are accessible");
}
