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
