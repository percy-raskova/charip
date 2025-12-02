//! Shared test utilities for charip-lsp.
//!
//! This module provides common helpers used across multiple test modules.
//! It is only compiled when running tests.

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::config::Settings;
use crate::vault::Vault;

/// Creates a temporary vault directory for testing.
///
/// Returns a tuple of (TempDir, PathBuf) where:
/// - TempDir: The temp directory handle (must be kept alive for the test duration)
/// - PathBuf: The path to the vault subdirectory
///
/// # Why this helper exists
///
/// The vault construction uses WalkDir which filters out hidden directories
/// (those starting with `.`). On some systems, temp directories are created
/// under paths like `/tmp/.tmpXXXXX`. By creating a non-hidden subdirectory
/// called "vault", we ensure the vault can properly index the test files.
///
/// # Example
///
/// ```ignore
/// use crate::test_utils::create_test_vault_dir;
///
/// let (_temp_dir, vault_dir) = create_test_vault_dir();
/// std::fs::write(vault_dir.join("test.md"), "# Test").unwrap();
/// ```
pub fn create_test_vault_dir() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    // Create a non-hidden subdirectory since WalkDir filters out .* dirs
    let vault_dir = temp_dir.path().join("vault");
    fs::create_dir(&vault_dir).expect("Failed to create vault subdirectory");
    (temp_dir, vault_dir)
}

/// Creates a test vault from a temporary directory.
///
/// This is a convenience function that combines `create_test_vault_dir`
/// with `Vault::construct_vault` using default settings.
///
/// # Arguments
///
/// * `setup_fn` - A closure that receives the vault directory path and can
///   create files before the vault is constructed.
///
/// # Returns
///
/// A tuple of (TempDir, PathBuf, Vault) where:
/// - TempDir: The temp directory handle (must be kept alive)
/// - PathBuf: The path to the vault directory
/// - Vault: The constructed vault instance
///
/// # Example
///
/// ```ignore
/// use crate::test_utils::create_test_vault;
///
/// let (_temp_dir, vault_dir, vault) = create_test_vault(|dir| {
///     std::fs::write(dir.join("test.md"), "# Test").unwrap();
/// });
/// ```
#[allow(dead_code)] // This helper is available for future tests
pub fn create_test_vault<F>(setup_fn: F) -> (TempDir, PathBuf, Vault)
where
    F: FnOnce(&PathBuf),
{
    let (temp_dir, vault_dir) = create_test_vault_dir();
    setup_fn(&vault_dir);
    let settings = Settings::default();
    let vault =
        Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct test vault");
    (temp_dir, vault_dir, vault)
}

/// Creates a generic vault structure for {doc} role integration testing.
///
/// This fixture creates a standardized directory structure with target files
/// that can be referenced via {doc} roles. Tests structure and syntax patterns,
/// not content.
///
/// # Vault Structure
///
/// ```text
/// vault/
/// ├── guides/
/// │   ├── getting-started.md
/// │   ├── installation.md
/// │   └── configuration.md
/// ├── reference/
/// │   ├── api-overview.md
/// │   ├── commands.md
/// │   └── options.md
/// ├── tutorials/
/// │   └── advanced/
/// │       ├── topic-one.md
/// │       └── topic-two.md
/// ├── file with spaces.md
/// ├── glossary.md
/// └── index.md
/// ```
///
/// # Returns
///
/// A tuple of (TempDir, PathBuf) where:
/// - TempDir: The temp directory handle (must be kept alive for test duration)
/// - PathBuf: The path to the vault directory
///
/// After calling this, tests should:
/// 1. Add their source files with {doc} roles
/// 2. Construct the vault using `Vault::construct_vault`
///
/// # Example
///
/// ```ignore
/// use crate::test_utils::create_doc_role_test_fixture;
///
/// let (_temp_dir, vault_dir) = create_doc_role_test_fixture();
///
/// // Add source file with {doc} role
/// std::fs::write(
///     vault_dir.join("source.md"),
///     "# Source\n\nSee {doc}`getting-started` for info.",
/// ).unwrap();
///
/// // Construct vault and run diagnostics
/// let settings = Settings { unresolved_diagnostics: true, ..Default::default() };
/// let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();
/// ```
#[allow(dead_code)] // This helper is available for integration tests
pub fn create_doc_role_test_fixture() -> (TempDir, PathBuf) {
    let (temp_dir, vault_dir) = create_test_vault_dir();

    // Create directory structure
    fs::create_dir(vault_dir.join("guides")).expect("Failed to create guides/");
    fs::create_dir(vault_dir.join("reference")).expect("Failed to create reference/");
    fs::create_dir_all(vault_dir.join("tutorials/advanced"))
        .expect("Failed to create tutorials/advanced/");

    // Create guides/ files
    fs::write(
        vault_dir.join("guides/getting-started.md"),
        "# Getting Started\n\nIntroduction content.",
    )
    .expect("Failed to write getting-started.md");
    fs::write(
        vault_dir.join("guides/installation.md"),
        "# Installation\n\nInstallation steps.",
    )
    .expect("Failed to write installation.md");
    fs::write(
        vault_dir.join("guides/configuration.md"),
        "# Configuration\n\nConfiguration options.",
    )
    .expect("Failed to write configuration.md");

    // Create reference/ files
    fs::write(
        vault_dir.join("reference/api-overview.md"),
        "# API Overview\n\nAPI documentation.",
    )
    .expect("Failed to write api-overview.md");
    fs::write(
        vault_dir.join("reference/commands.md"),
        "# Commands\n\nCommand reference.",
    )
    .expect("Failed to write commands.md");
    fs::write(
        vault_dir.join("reference/options.md"),
        "# Options\n\nConfiguration options.",
    )
    .expect("Failed to write options.md");

    // Create tutorials/advanced/ files
    fs::write(
        vault_dir.join("tutorials/advanced/topic-one.md"),
        "# Topic One\n\nAdvanced topic one.",
    )
    .expect("Failed to write topic-one.md");
    fs::write(
        vault_dir.join("tutorials/advanced/topic-two.md"),
        "# Topic Two\n\nAdvanced topic two.",
    )
    .expect("Failed to write topic-two.md");

    // Create root files
    fs::write(
        vault_dir.join("file with spaces.md"),
        "# File With Spaces\n\nContent with spaces in filename.",
    )
    .expect("Failed to write 'file with spaces.md'");
    fs::write(
        vault_dir.join("glossary.md"),
        "# Glossary\n\nTerms and definitions.",
    )
    .expect("Failed to write glossary.md");
    fs::write(vault_dir.join("index.md"), "# Index\n\nMain index page.")
        .expect("Failed to write index.md");

    (temp_dir, vault_dir)
}
