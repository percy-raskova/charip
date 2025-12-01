//! Helper types and utilities for the vault module.

use std::ops::Deref;
use std::path::Path;

use pathdiff::diff_paths;

/// Represents a reference name that may contain a path and/or infile reference.
#[derive(Debug, PartialEq, Eq, Default)]
pub struct Refname {
    pub full_refname: String,
    pub path: Option<String>,
    pub infile_ref: Option<String>,
}

impl Refname {
    pub fn link_file_key(&self) -> Option<String> {
        let path = &self.path.clone()?;

        let last = path.split('/').next_back()?;

        Some(last.to_string())
    }
}

impl Deref for Refname {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.full_refname
    }
}

impl From<String> for Refname {
    fn from(value: String) -> Self {
        Refname {
            full_refname: value.clone(),
            ..Default::default()
        }
    }
}

impl From<&str> for Refname {
    fn from(value: &str) -> Self {
        Refname {
            full_refname: value.to_string(),
            ..Default::default()
        }
    }
}

/// Utility function to get the Obsidian-style reference path from a file path.
pub fn get_obsidian_ref_path(root_dir: &Path, path: &Path) -> Option<String> {
    diff_paths(path, root_dir).and_then(|diff| diff.with_extension("").to_str().map(String::from))
}
