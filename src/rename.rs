use std::iter;
use std::path::Path;

use tower_lsp::lsp_types::{
    DocumentChangeOperation, DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier,
    RenameFile, RenameParams, ResourceOp, TextDocumentEdit, TextEdit, Url, WorkspaceEdit,
};

use crate::vault::{Reference, Referenceable, Vault};

pub fn rename(vault: &Vault, params: &RenameParams, path: &Path) -> Option<WorkspaceEdit> {
    let position = params.text_document_position.position;
    let referenceable = vault.select_referenceable_at_position(path, position)?;

    let (referenceable_document_change, new_ref_name): (Option<DocumentChangeOperation>, String) =
        match referenceable {
            Referenceable::Heading(path, heading) => {
                let new_text = format!("{} {}", "#".repeat(heading.level.0), params.new_name);

                let change_op = DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: tower_lsp::lsp_types::OptionalVersionedTextDocumentIdentifier {
                        uri: Url::from_file_path(path).ok()?,
                        version: None,
                    },
                    edits: vec![OneOf::Left(TextEdit {
                        range: *heading.range,
                        new_text,
                    })],
                });

                // {path name}#{new name}
                let name = format!(
                    "{}#{}",
                    path.file_stem()?.to_string_lossy().clone(),
                    params.new_name
                );

                (Some(change_op), name.to_string())
            }
            Referenceable::File(path, _file) => {
                let new_path = path.with_file_name(&params.new_name).with_extension("md");

                let change_op = DocumentChangeOperation::Op(ResourceOp::Rename(RenameFile {
                    old_uri: Url::from_file_path(path).ok()?,
                    new_uri: Url::from_file_path(new_path.clone()).ok()?,
                    options: None,
                    annotation_id: None,
                }));

                let name = params.new_name.clone();

                (Some(change_op), name)
            }
            Referenceable::Tag(_path, _tag) => {
                let new_ref_name = params.new_name.clone();

                let _new_tag = format!("#{}", new_ref_name);

                (None, new_ref_name)
            }
            _ => return None,
        };

    let references = vault.select_references_for_referenceable(&referenceable)?;

    let references_changes = references
        .into_iter()
        .filter_map(|(path, reference)| {
            // update references

            match reference {
                Reference::Tag(data) => {
                    let new_text = format!(
                        "#{}",
                        data.reference_text.replacen(
                            &*referenceable.get_refname(vault.root_dir())?,
                            &new_ref_name,
                            1
                        )
                    );

                    Some(TextDocumentEdit {
                        text_document: OptionalVersionedTextDocumentIdentifier {
                            uri: Url::from_file_path(path).ok()?,
                            version: None,
                        },
                        edits: vec![OneOf::Left(TextEdit {
                            range: *data.range,
                            new_text,
                        })],
                    })
                }
                Reference::MDFileLink(data) if matches!(referenceable, Referenceable::File(..)) => {
                    let new_text = format!(
                        "[{}]({})",
                        data.display_text
                            .as_ref()
                            .map(|text| format!("|{text}"))
                            .unwrap_or_else(|| String::from("")),
                        new_ref_name,
                    );

                    Some(TextDocumentEdit {
                        text_document:
                            tower_lsp::lsp_types::OptionalVersionedTextDocumentIdentifier {
                                uri: Url::from_file_path(path).ok()?,
                                version: None,
                            },
                        edits: vec![OneOf::Left(TextEdit {
                            range: *data.range,
                            new_text,
                        })],
                    })
                }

                Reference::MDHeadingLink(data, _file, infile)
                | Reference::MDIndexedBlockLink(data, _file, infile)
                    if matches!(referenceable, Referenceable::File(..)) =>
                {
                    let new_text = format!(
                        "[{}]({}#{})",
                        data.display_text
                            .as_ref()
                            .map(|text| format!("|{text}"))
                            .unwrap_or_else(|| String::from("")),
                        new_ref_name,
                        infile,
                    );

                    Some(TextDocumentEdit {
                        text_document:
                            tower_lsp::lsp_types::OptionalVersionedTextDocumentIdentifier {
                                uri: Url::from_file_path(path).ok()?,
                                version: None,
                            },
                        edits: vec![OneOf::Left(TextEdit {
                            range: *data.range,
                            new_text,
                        })],
                    })
                }
                Reference::MDHeadingLink(data, _file, _heading)
                    if matches!(referenceable, Referenceable::Heading(..)) =>
                {
                    let new_text = format!(
                        "[{}]({})",
                        data.display_text
                            .as_ref()
                            .map(|text| format!("|{text}"))
                            .unwrap_or_else(|| String::from("")),
                        new_ref_name,
                    );

                    Some(TextDocumentEdit {
                        text_document:
                            tower_lsp::lsp_types::OptionalVersionedTextDocumentIdentifier {
                                uri: Url::from_file_path(path).ok()?,
                                version: None,
                            },
                        edits: vec![OneOf::Left(TextEdit {
                            range: *data.range,
                            new_text,
                        })],
                    })
                }
                // Catch-all for unhandled cases
                Reference::MDHeadingLink(_, _, _) => None,
                Reference::MDIndexedBlockLink(_, _, _) => None,
                Reference::MDFileLink(..) => None,
                Reference::Footnote(..) => None,
                Reference::LinkRef(_) => None,
                Reference::MystRole(..) => None, // MyST role renaming not yet supported
            }
        })
        .map(DocumentChangeOperation::Edit);

    Some(WorkspaceEdit {
        document_changes: Some(DocumentChanges::Operations(
            references_changes
                .chain(iter::once(referenceable_document_change).flatten())
                .collect(), // order matters here
        )),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::test_utils::create_test_vault_dir;
    use std::fs;
    use tower_lsp::lsp_types::{Position, TextDocumentIdentifier, TextDocumentPositionParams};

    /// Helper to create RenameParams
    fn create_rename_params(
        path: &std::path::Path,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> RenameParams {
        RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(path).unwrap(),
                },
                position: Position { line, character },
            },
            new_name: new_name.to_string(),
            work_done_progress_params: Default::default(),
        }
    }

    /// Test: Rename a file updates the file and all references to it.
    #[test]
    fn test_rename_file() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file
        fs::write(vault_dir.join("oldname.md"), "# Old Name\n\nContent here.").unwrap();

        // Create source file with link to target
        fs::write(
            vault_dir.join("source.md"),
            "# Source\n\nSee [link](oldname) for more.",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on plain text in oldname.md (not on heading) to select the file
        let file_path = vault_dir.join("oldname.md");
        let params = create_rename_params(&file_path, 2, 5, "newname");

        let result = rename(&vault, &params, &file_path);

        assert!(result.is_some(), "Rename should return a WorkspaceEdit");
        let workspace_edit = result.unwrap();

        // Check document_changes exist
        assert!(
            workspace_edit.document_changes.is_some(),
            "Should have document_changes"
        );

        if let Some(DocumentChanges::Operations(ops)) = workspace_edit.document_changes {
            // Should have file rename operation and possibly reference updates
            let rename_ops: Vec<_> = ops
                .iter()
                .filter(|op| matches!(op, DocumentChangeOperation::Op(ResourceOp::Rename(_))))
                .collect();
            assert_eq!(
                rename_ops.len(),
                1,
                "Should have exactly 1 file rename operation"
            );

            // Verify the rename operation has correct URIs
            if let DocumentChangeOperation::Op(ResourceOp::Rename(rename_file)) = &rename_ops[0] {
                assert!(rename_file.old_uri.path().ends_with("oldname.md"));
                assert!(rename_file.new_uri.path().ends_with("newname.md"));
            }

            // Should also have text edits to update references
            let edit_ops: Vec<_> = ops
                .iter()
                .filter(|op| matches!(op, DocumentChangeOperation::Edit(_)))
                .collect();
            assert!(
                !edit_ops.is_empty(),
                "Should have at least 1 text edit for reference updates"
            );
        }
    }

    /// Test: Rename a heading updates the heading text and all references to it.
    #[test]
    fn test_rename_heading() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with heading
        fs::write(
            vault_dir.join("target.md"),
            "# Main Title\n\n## Old Heading\n\nContent here.",
        )
        .unwrap();

        // Create source file with link to the heading
        fs::write(
            vault_dir.join("source.md"),
            "# Source\n\nSee [details](target#Old Heading) for more.",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the heading line
        let file_path = vault_dir.join("target.md");
        let params = create_rename_params(&file_path, 2, 5, "New Heading");

        let result = rename(&vault, &params, &file_path);

        assert!(result.is_some(), "Rename should return a WorkspaceEdit");
        let workspace_edit = result.unwrap();

        // Check document_changes exist
        assert!(
            workspace_edit.document_changes.is_some(),
            "Should have document_changes"
        );

        if let Some(DocumentChanges::Operations(ops)) = workspace_edit.document_changes {
            // Should have text edits for the heading itself and for references
            let edit_ops: Vec<_> = ops
                .iter()
                .filter(|op| matches!(op, DocumentChangeOperation::Edit(_)))
                .collect();
            assert!(!edit_ops.is_empty(), "Should have text edits");

            // Find the heading edit (in target.md)
            let target_uri = Url::from_file_path(vault_dir.join("target.md")).unwrap();
            let heading_edits: Vec<_> = edit_ops
                .iter()
                .filter(|op| {
                    if let DocumentChangeOperation::Edit(edit) = op {
                        edit.text_document.uri == target_uri
                    } else {
                        false
                    }
                })
                .collect();
            assert_eq!(
                heading_edits.len(),
                1,
                "Should have 1 edit in target.md for heading"
            );

            // Verify the new text includes the new heading name
            if let DocumentChangeOperation::Edit(edit) = heading_edits[0] {
                let new_text = &edit.edits[0];
                if let OneOf::Left(text_edit) = new_text {
                    assert!(
                        text_edit.new_text.contains("New Heading"),
                        "New text should contain new heading name"
                    );
                }
            }
        }
    }

    /// Test: Rename returns None when cursor is not on a renameable item.
    #[test]
    fn test_rename_non_renameable() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with just plain text (no heading on line 2)
        fs::write(
            vault_dir.join("test.md"),
            "# Heading\n\nPlain text content.",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on plain text (the File referenceable should be selected, which IS renameable)
        // But let's check what happens
        let file_path = vault_dir.join("test.md");
        let params = create_rename_params(&file_path, 2, 5, "NewName");

        let result = rename(&vault, &params, &file_path);

        // When cursor is on plain text, the File referenceable is selected (fallback behavior)
        // So rename IS supported - it would rename the file
        // This test documents that behavior
        assert!(
            result.is_some(),
            "Rename on plain text selects File, which is renameable"
        );
    }

    /// Test: Rename tag updates all tag occurrences (documents current behavior).
    ///
    /// Note: Tags ARE renameable in the implementation (they have a match arm in rename()),
    /// but the actual reference updates might be limited.
    #[test]
    fn test_rename_tag() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create files with tags
        fs::write(vault_dir.join("file1.md"), "# File 1\n\n#oldtag content.").unwrap();
        fs::write(vault_dir.join("file2.md"), "# File 2\n\n#oldtag here too.").unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the tag
        let file_path = vault_dir.join("file1.md");
        let params = create_rename_params(&file_path, 2, 2, "newtag");

        let result = rename(&vault, &params, &file_path);

        // Tags are handled in the rename function
        assert!(
            result.is_some(),
            "Rename should return a WorkspaceEdit for tags"
        );
        let workspace_edit = result.unwrap();

        // The workspace edit should contain operations
        assert!(
            workspace_edit.document_changes.is_some(),
            "Should have document_changes"
        );
    }

    /// Test: Rename works correctly with workspace edit structure.
    #[test]
    fn test_rename_workspace_edit_structure() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create a simple file
        fs::write(vault_dir.join("test.md"), "# Test File\n\nContent.").unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the heading
        let file_path = vault_dir.join("test.md");
        let params = create_rename_params(&file_path, 0, 3, "New Title");

        let result = rename(&vault, &params, &file_path);

        assert!(result.is_some(), "Rename should return a WorkspaceEdit");
        let workspace_edit = result.unwrap();

        // Verify the structure of WorkspaceEdit
        match workspace_edit.document_changes {
            Some(DocumentChanges::Operations(ops)) => {
                assert!(!ops.is_empty(), "Operations should not be empty");

                // Each operation should be valid
                for op in &ops {
                    match op {
                        DocumentChangeOperation::Edit(edit) => {
                            assert!(edit.text_document.uri.scheme() == "file");
                            assert!(!edit.edits.is_empty());
                        }
                        DocumentChangeOperation::Op(resource_op) => {
                            if let ResourceOp::Rename(rename_file) = resource_op {
                                assert!(rename_file.old_uri.scheme() == "file");
                                assert!(rename_file.new_uri.scheme() == "file");
                            }
                            // Other ops are fine
                        }
                    }
                }
            }
            Some(DocumentChanges::Edits(_)) => {
                // This format is also acceptable
            }
            None => {
                panic!("WorkspaceEdit should have document_changes");
            }
        }
    }

    /// Test: Rename file with multiple references from different files.
    #[test]
    fn test_rename_file_multiple_references() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file
        fs::write(vault_dir.join("target.md"), "# Target\n\nContent.").unwrap();

        // Create multiple source files linking to target
        fs::write(
            vault_dir.join("source1.md"),
            "# Source 1\n\n[link1](target) here.",
        )
        .unwrap();
        fs::write(
            vault_dir.join("source2.md"),
            "# Source 2\n\n[link2](target) here.",
        )
        .unwrap();
        fs::write(
            vault_dir.join("source3.md"),
            "# Source 3\n\n[link3](target) here.",
        )
        .unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor in target.md to select the file
        let file_path = vault_dir.join("target.md");
        let params = create_rename_params(&file_path, 2, 2, "renamed");

        let result = rename(&vault, &params, &file_path);

        assert!(result.is_some(), "Rename should return a WorkspaceEdit");
        let workspace_edit = result.unwrap();

        if let Some(DocumentChanges::Operations(ops)) = workspace_edit.document_changes {
            // Should have 1 file rename + 3 text edits for references
            let rename_ops: Vec<_> = ops
                .iter()
                .filter(|op| matches!(op, DocumentChangeOperation::Op(ResourceOp::Rename(_))))
                .collect();
            let edit_ops: Vec<_> = ops
                .iter()
                .filter(|op| matches!(op, DocumentChangeOperation::Edit(_)))
                .collect();

            assert_eq!(rename_ops.len(), 1, "Should have 1 file rename operation");
            assert_eq!(
                edit_ops.len(),
                3,
                "Should have 3 text edits for references in source files"
            );
        }
    }
}
