//! charip-lsp: A MyST-aware Language Server Protocol implementation
//!
//! This crate provides the core functionality for the charip LSP server,
//! enabling intelligent editing support for MyST (Markedly Structured Text)
//! documents in knowledge vaults.
//!
//! # Overview
//!
//! charip-lsp is designed for MyST/Sphinx documentation projects, providing:
//!
//! - **Vault Management**: Graph-based indexing of documentation files
//! - **Reference Resolution**: Go-to-definition and find-references for MyST cross-references
//! - **Autocomplete**: Context-aware completions for links, roles, directives, and tags
//! - **Diagnostics**: Broken link detection and validation
//! - **Rename Support**: Safe refactoring across the vault
//!
//! # Architecture
//!
//! The crate is organized around several key modules:
//!
//! - [`vault`]: Core data structures for the knowledge graph (petgraph-based)
//! - [`completion`]: Autocomplete providers for various MyST constructs
//! - [`myst_parser`]: Extraction of MyST-specific syntax (roles, directives, anchors)
//! - [`config`]: Configuration management and settings
//!
//! # Usage
//!
//! This crate is primarily used as the backing library for the `charip` binary,
//! which implements the LSP server. The public API enables programmatic access
//! to vault operations and analysis.
//!
//! ```ignore
//! use charip::vault::Vault;
//! use charip::config::Settings;
//!
//! let settings = Settings::default();
//! let vault = Vault::construct_vault(&settings, &vault_path)?;
//! ```

// Core modules - vault and graph structure
pub mod vault;

// LSP feature modules
pub mod codeactions;
pub mod codelens;
pub mod commands;
pub mod completion;
pub mod diagnostics;
pub mod gotodef;
pub mod hover;
pub mod references;
pub mod rename;
pub mod symbol;

// Configuration and parsing
pub mod config;
pub mod frontmatter_schema;
pub mod myst_parser;

// Utilities
pub mod cli;
pub mod daily;
pub mod tokens;
pub mod ui;

// Internal macros (used across modules)
#[macro_use]
mod macros;

// Test utilities (only available in test builds)
#[cfg(test)]
pub mod test_utils;
