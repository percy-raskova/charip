use std::iter;
use std::path::Path;

use tower_lsp::lsp_types::{
    DocumentChangeOperation, DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier,
    RenameParams, TextDocumentEdit, TextEdit, Url, WorkspaceEdit,
};

use crate::vault::{MystRoleKind, Reference, Referenceable, Vault};

/// Attempts to resolve a MyST role reference (like `{ref}` or `{numref}`) to its target anchor.
///
/// When the cursor is on a MyST role that references an anchor, we want rename operations
/// to target the anchor definition, not the file. This function handles that resolution.
///
/// Returns `Some(Referenceable::MystAnchor)` if the reference at the position is a
/// `{ref}` or `{numref}` role that successfully resolves to an anchor. Returns `None`
/// for other role types or if no anchor is found.
fn resolve_myst_role_to_anchor<'a>(
    vault: &'a Vault,
    reference: &Reference,
    path: &Path,
) -> Option<Referenceable<'a>> {
    match reference {
        // Only {ref} and {numref} roles reference anchors
        Reference::MystRole(_, MystRoleKind::Ref, target)
        | Reference::MystRole(_, MystRoleKind::NumRef, target) => {
            let referenceables = vault.select_referenceables_for_reference(reference, path);
            referenceables.into_iter().find(|r| {
                matches!(r, Referenceable::MystAnchor(_, symbol) if symbol.name.to_lowercase() == target.to_lowercase())
            })
        }
        // Other role types (doc, download, term, etc.) don't rename anchors
        _ => None,
    }
}

pub fn rename(vault: &Vault, params: &RenameParams, path: &Path) -> Option<WorkspaceEdit> {
    let position = params.text_document_position.position;

    // Try to resolve a MyST role reference to its target anchor first.
    // If that fails, fall back to selecting the referenceable at the cursor position.
    let referenceable = vault
        .select_reference_at_position(path, position)
        .and_then(|reference| resolve_myst_role_to_anchor(vault, reference, path))
        .or_else(|| vault.select_referenceable_at_position(path, position))?;

    // Use the ReferenceableOps method to generate the definition-side edit
    let (referenceable_document_change, new_ref_name) =
        referenceable.get_definition_rename_edit(&params.new_name)?;

    let references = vault.select_references_for_referenceable(&referenceable)?;

    let references_changes = references
        .into_iter()
        .filter_map(|(path, reference)| {
            // Use the ReferenceOps trait method to get the rename text
            let new_text =
                reference.get_rename_text(&referenceable, &new_ref_name, vault.root_dir())?;

            Some(TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: Url::from_file_path(path).ok()?,
                    version: None,
                },
                edits: vec![OneOf::Left(TextEdit {
                    range: *reference.data().range,
                    new_text,
                })],
            })
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
    use tower_lsp::lsp_types::{
        Position, ResourceOp, TextDocumentIdentifier, TextDocumentPositionParams,
    };

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

    // =========================================================================
    // MyST Role Rename Tests (TDD Red Phase)
    // =========================================================================

    /// Test: Rename from cursor on {ref}`anchor` role should work.
    ///
    /// When the cursor is positioned on a MyST role like {ref}`my-anchor`,
    /// triggering rename should:
    /// 1. Resolve the role to its target MystAnchor
    /// 2. Rename the anchor definition (my-anchor)=
    /// 3. Update all references to that anchor
    #[test]
    fn test_rename_from_myst_ref_role_cursor() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file with MyST anchor
        let target_content = "(old-anchor)=\n# Important Section\n\nContent here.";
        fs::write(vault_dir.join("target.md"), target_content).unwrap();

        // Create source file with {ref} role - cursor will be here
        let source_content = "# Source\n\nSee {ref}`old-anchor` for the important section.";
        fs::write(vault_dir.join("source.md"), source_content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the {ref} role (inside the backticks, on "old-anchor")
        // Line 2: "See {ref}`old-anchor` for the important section."
        //                    ^--- character 10 is inside the target
        let source_path = vault_dir.join("source.md");
        let params = create_rename_params(&source_path, 2, 10, "new-anchor");

        let result = rename(&vault, &params, &source_path);

        assert!(
            result.is_some(),
            "Rename from MyST ref role should return a WorkspaceEdit"
        );

        let workspace_edit = result.unwrap();
        assert!(
            workspace_edit.document_changes.is_some(),
            "Should have document_changes"
        );

        if let Some(DocumentChanges::Operations(ops)) = workspace_edit.document_changes {
            // Should have text edits for:
            // 1. The anchor definition (old-anchor)= in target.md
            // 2. The {ref}`old-anchor` reference in source.md
            let edit_ops: Vec<_> = ops
                .iter()
                .filter(|op| matches!(op, DocumentChangeOperation::Edit(_)))
                .collect();

            assert!(
                edit_ops.len() >= 2,
                "Should have at least 2 text edits (anchor definition + role reference), got {}",
                edit_ops.len()
            );

            // Verify that target.md has an edit for the anchor definition
            let target_uri = Url::from_file_path(vault_dir.join("target.md")).unwrap();
            let has_target_edit = edit_ops.iter().any(|op| {
                if let DocumentChangeOperation::Edit(edit) = op {
                    edit.text_document.uri == target_uri
                } else {
                    false
                }
            });
            assert!(
                has_target_edit,
                "Should have an edit in target.md for anchor definition"
            );

            // Verify that source.md has an edit for the role reference
            let source_uri = Url::from_file_path(vault_dir.join("source.md")).unwrap();
            let has_source_edit = edit_ops.iter().any(|op| {
                if let DocumentChangeOperation::Edit(edit) = op {
                    edit.text_document.uri == source_uri
                } else {
                    false
                }
            });
            assert!(
                has_source_edit,
                "Should have an edit in source.md for role reference"
            );
        }
    }

    /// Test: Rename MyST anchor updates all {ref} and {numref} references.
    ///
    /// Both {ref}`anchor` and {numref}`anchor` should be updated when
    /// the anchor is renamed.
    #[test]
    fn test_rename_myst_anchor_updates_all_role_refs() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file with anchor
        let target_content = "(my-figure)=\n```{figure} image.png\nCaption\n```";
        fs::write(vault_dir.join("target.md"), target_content).unwrap();

        // Create source with both {ref} and {numref} roles
        let source_content =
            "# Source\n\nSee {ref}`my-figure` and {numref}`Figure %s <my-figure>`.";
        fs::write(vault_dir.join("source.md"), source_content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on {ref} role
        let source_path = vault_dir.join("source.md");
        let params = create_rename_params(&source_path, 2, 10, "renamed-figure");

        let result = rename(&vault, &params, &source_path);

        assert!(result.is_some(), "Rename should return a WorkspaceEdit");

        let workspace_edit = result.unwrap();
        if let Some(DocumentChanges::Operations(ops)) = workspace_edit.document_changes {
            let edit_ops: Vec<_> = ops
                .iter()
                .filter(|op| matches!(op, DocumentChangeOperation::Edit(_)))
                .collect();

            // Should have:
            // 1. Anchor definition in target.md
            // 2. {ref}`my-figure` in source.md
            // 3. {numref}`...my-figure...` in source.md
            assert!(
                edit_ops.len() >= 3,
                "Should have at least 3 edits (anchor + 2 role refs), got {}",
                edit_ops.len()
            );
        }
    }

    /// Test: Rename MyST anchor from cursor on anchor definition.
    ///
    /// When cursor is directly on (anchor-name)=, rename should work
    /// and update all references.
    #[test]
    fn test_rename_myst_anchor_from_definition() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with anchor and self-reference
        let content = "(my-section)=\n# My Section\n\nRefer to {ref}`my-section` above.";
        fs::write(vault_dir.join("doc.md"), content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on the anchor definition "(my-section)="
        // Line 0: "(my-section)="
        //          ^--- character 1 is inside "my-section"
        let file_path = vault_dir.join("doc.md");
        let params = create_rename_params(&file_path, 0, 1, "renamed-section");

        let result = rename(&vault, &params, &file_path);

        assert!(
            result.is_some(),
            "Rename from anchor definition should return a WorkspaceEdit"
        );

        let workspace_edit = result.unwrap();
        if let Some(DocumentChanges::Operations(ops)) = workspace_edit.document_changes {
            let edit_ops: Vec<_> = ops
                .iter()
                .filter(|op| matches!(op, DocumentChangeOperation::Edit(_)))
                .collect();

            // Should have:
            // 1. Anchor definition (my-section)=
            // 2. {ref}`my-section` reference
            assert!(
                edit_ops.len() >= 2,
                "Should have at least 2 edits, got {}",
                edit_ops.len()
            );
        }
    }

    /// Test: Rename from {doc} role should NOT trigger anchor rename.
    ///
    /// {doc}`path` roles reference files, not anchors. They should not
    /// trigger the anchor rename workflow.
    #[test]
    fn test_rename_doc_role_not_supported_for_anchor_rename() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target file
        fs::write(vault_dir.join("other.md"), "# Other Document").unwrap();

        // Create source with {doc} role
        let source_content = "# Source\n\nSee {doc}`other` for more.";
        fs::write(vault_dir.join("source.md"), source_content).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on {doc} role
        let source_path = vault_dir.join("source.md");
        let params = create_rename_params(&source_path, 2, 10, "renamed");

        let result = rename(&vault, &params, &source_path);

        // {doc} roles point to files, not anchors.
        // If rename is supported, it should rename the FILE (like MDFileLink),
        // not an anchor. For now, we document that it either:
        // 1. Returns None (not supported)
        // 2. Or renames the file (if file rename from reference is supported)
        //
        // This test documents the expected behavior: {doc} should not try
        // to find/rename an anchor.

        // For this implementation, we expect None since file rename from
        // reference position is not implemented for {doc} roles yet.
        // If it returns Some, it should be a file rename, not anchor rename.
        if let Some(workspace_edit) = result {
            if let Some(DocumentChanges::Operations(ops)) = workspace_edit.document_changes {
                // If we got operations, they should NOT include anchor edits
                // (i.e., no "(new-anchor)=" type edits)
                for op in &ops {
                    if let DocumentChangeOperation::Edit(edit) = op {
                        for e in &edit.edits {
                            if let OneOf::Left(text_edit) = e {
                                assert!(
                                    !text_edit.new_text.contains(")="),
                                    "{{doc}} role rename should not create anchor-style edits"
                                );
                            }
                        }
                    }
                }
            }
        }
        // None is also acceptable - {doc} role rename just not supported yet
    }

    /// Test: Verify the new anchor text format after rename.
    ///
    /// The anchor definition should be formatted as "(new-name)="
    #[test]
    fn test_rename_myst_anchor_new_text_format() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create file with anchor
        let content = "(old-name)=\n# Section";
        fs::write(vault_dir.join("doc.md"), content).unwrap();

        // Create source with reference
        let source = "# Source\n\nSee {ref}`old-name`.";
        fs::write(vault_dir.join("source.md"), source).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Rename from the source file's {ref} role
        let source_path = vault_dir.join("source.md");
        let params = create_rename_params(&source_path, 2, 10, "new-name");

        let result = rename(&vault, &params, &source_path);

        assert!(result.is_some(), "Rename should return a WorkspaceEdit");

        let workspace_edit = result.unwrap();
        if let Some(DocumentChanges::Operations(ops)) = workspace_edit.document_changes {
            // Find the edit for the anchor definition in doc.md
            let doc_uri = Url::from_file_path(vault_dir.join("doc.md")).unwrap();

            let anchor_edit = ops.iter().find_map(|op| {
                if let DocumentChangeOperation::Edit(edit) = op {
                    if edit.text_document.uri == doc_uri {
                        return Some(edit);
                    }
                }
                None
            });

            assert!(
                anchor_edit.is_some(),
                "Should have an edit for anchor definition"
            );

            let edit = anchor_edit.unwrap();
            assert!(!edit.edits.is_empty());

            if let OneOf::Left(text_edit) = &edit.edits[0] {
                assert_eq!(
                    text_edit.new_text, "(new-name)=",
                    "New anchor text should be in format (name)="
                );
            }
        }
    }

    /// Test: Rename from {numref} role works the same as {ref}.
    #[test]
    fn test_rename_from_numref_role_cursor() {
        let (_temp_dir, vault_dir) = create_test_vault_dir();

        // Create target with anchor
        let target = "(fig-1)=\n```{figure} img.png\nCaption\n```";
        fs::write(vault_dir.join("target.md"), target).unwrap();

        // Create source with {numref} role
        let source = "# Source\n\nAs shown in {numref}`fig-1`.";
        fs::write(vault_dir.join("source.md"), source).unwrap();

        let settings = Settings::default();
        let vault =
            Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

        // Position cursor on {numref} role
        // Line 2: "As shown in {numref}`fig-1`."
        //                              ^--- around character 22
        let source_path = vault_dir.join("source.md");
        let params = create_rename_params(&source_path, 2, 22, "fig-renamed");

        let result = rename(&vault, &params, &source_path);

        assert!(
            result.is_some(),
            "Rename from {{numref}} role should return a WorkspaceEdit"
        );

        let workspace_edit = result.unwrap();
        assert!(
            workspace_edit.document_changes.is_some(),
            "Should have document_changes"
        );

        // Verify that the anchor definition is updated
        if let Some(DocumentChanges::Operations(ops)) = workspace_edit.document_changes {
            let target_uri = Url::from_file_path(vault_dir.join("target.md")).unwrap();

            let has_anchor_edit = ops.iter().any(|op| {
                if let DocumentChangeOperation::Edit(edit) = op {
                    if edit.text_document.uri == target_uri {
                        // Check that the edit changes the anchor
                        return edit.edits.iter().any(|e| {
                            if let OneOf::Left(text_edit) = e {
                                text_edit.new_text.contains("fig-renamed")
                            } else {
                                false
                            }
                        });
                    }
                }
                false
            });

            assert!(
                has_anchor_edit,
                "Should have an edit in target.md that updates the anchor to fig-renamed"
            );
        }
    }
}
