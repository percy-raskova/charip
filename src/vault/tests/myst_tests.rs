use crate::config::Settings;
use crate::myst_parser::MystSymbolKind;
use crate::vault::*;
use std::fs;
use tempfile::TempDir;

/// Helper to create a test vault directory (avoids hidden .tmp dirs)
fn create_test_vault_dir() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    // Create a non-hidden subdirectory since WalkDir filters out .* dirs
    let vault_dir = temp_dir.path().join("vault");
    fs::create_dir(&vault_dir).unwrap();
    (temp_dir, vault_dir)
}

#[test]
fn test_vault_extracts_myst_directives() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();
    let content = r#"
# My Document

```{note}
This is a note.
```

```{warning}
Be careful!
```
"#;
    let test_file = vault_dir.join("test.md");
    fs::write(&test_file, content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Use the convenience method instead of manual filtering
    let directives = vault.select_myst_directives(None);

    assert_eq!(directives.len(), 2, "Should find 2 directives");
}

#[test]
fn test_vault_extracts_myst_anchors() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();
    let content = "(my-anchor)=\n# Important Section";
    fs::write(vault_dir.join("test.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Use the convenience method instead of manual filtering
    let anchors = vault.select_myst_anchors(None);

    assert_eq!(anchors.len(), 1, "Should find 1 anchor");
    assert_eq!(anchors[0].1.name, "my-anchor");
}

#[test]
fn test_vault_finds_anchors_across_files() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    fs::write(
        vault_dir.join("chapter1.md"),
        "(chapter-1-intro)=\n# Chapter 1",
    )
    .unwrap();

    fs::write(
        vault_dir.join("chapter2.md"),
        "(chapter-2-summary)=\n# Chapter 2",
    )
    .unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Use the convenience method instead of manual filtering
    let anchors = vault.select_myst_anchors(None);

    assert_eq!(anchors.len(), 2, "Should find 2 anchors across files");
}

#[test]
fn test_select_myst_directives_with_path_filter() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // File with directives
    fs::write(
        vault_dir.join("with_directives.md"),
        "```{note}\nA note\n```\n\n```{warning}\nA warning\n```",
    )
    .unwrap();

    // File without directives
    fs::write(
        vault_dir.join("plain.md"),
        "# Just a heading\n\nSome plain text.",
    )
    .unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Get all directives
    let all_directives = vault.select_myst_directives(None);
    assert_eq!(all_directives.len(), 2, "Should find 2 directives total");

    // Get directives from specific file
    let file_path = vault_dir.join("with_directives.md");
    let file_directives = vault.select_myst_directives(Some(&file_path));
    assert_eq!(file_directives.len(), 2, "Should find 2 directives in file");

    // Get directives from file without any
    let plain_path = vault_dir.join("plain.md");
    let plain_directives = vault.select_myst_directives(Some(&plain_path));
    assert_eq!(
        plain_directives.len(),
        0,
        "Should find 0 directives in plain file"
    );
}

#[test]
fn test_select_myst_anchors_with_path_filter() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // File with anchors
    fs::write(
        vault_dir.join("with_anchors.md"),
        "(anchor-one)=\n# Section One\n\n(anchor-two)=\n# Section Two",
    )
    .unwrap();

    // File without anchors
    fs::write(vault_dir.join("no_anchors.md"), "# Just a heading").unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Get all anchors
    let all_anchors = vault.select_myst_anchors(None);
    assert_eq!(all_anchors.len(), 2, "Should find 2 anchors total");

    // Get anchors from specific file
    let file_path = vault_dir.join("with_anchors.md");
    let file_anchors = vault.select_myst_anchors(Some(&file_path));
    assert_eq!(file_anchors.len(), 2, "Should find 2 anchors in file");

    // Get anchors from file without any
    let no_anchor_path = vault_dir.join("no_anchors.md");
    let no_anchors = vault.select_myst_anchors(Some(&no_anchor_path));
    assert_eq!(no_anchors.len(), 0, "Should find 0 anchors in plain file");
}

#[test]
fn test_convenience_methods_return_correct_directive_names() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();
    let content = r#"
```{note}
Note content
```

```{admonition} Custom Title
:class: tip
Admonition content
```

```{code-block} python
print("hello")
```
"#;
    fs::write(vault_dir.join("test.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    let directives = vault.select_myst_directives(None);
    let names: Vec<_> = directives.iter().map(|(_, s)| s.name.as_str()).collect();

    assert!(names.contains(&"note"), "Should contain 'note' directive");
    assert!(
        names.contains(&"admonition"),
        "Should contain 'admonition' directive"
    );
    assert!(
        names.contains(&"code-block"),
        "Should contain 'code-block' directive"
    );
}

#[test]
fn test_myst_anchor_as_referenceable() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    let content = "(my-target)=\n# Section Title\n\nSome content.";
    fs::write(vault_dir.join("test.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Get all referenceables and check MystAnchor is included
    let referenceables = vault.select_referenceable_nodes(None);

    let anchor_refs: Vec<_> = referenceables
        .iter()
        .filter(|r| matches!(r, Referenceable::MystAnchor(..)))
        .collect();

    assert_eq!(anchor_refs.len(), 1, "Should find 1 MystAnchor referenceable");

    if let Referenceable::MystAnchor(_, symbol) = anchor_refs[0] {
        assert_eq!(symbol.name, "my-target");
        assert_eq!(symbol.kind, MystSymbolKind::Anchor);
    } else {
        panic!("Expected MystAnchor");
    }
}

#[test]
fn test_myst_ref_role_resolves_to_anchor() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // File with anchor
    let content_with_anchor = "(my-section)=\n# My Section\n\nContent here.";
    fs::write(vault_dir.join("target.md"), content_with_anchor).unwrap();

    // File with {ref} role pointing to that anchor
    let content_with_ref = "See {ref}`my-section` for more info.";
    fs::write(vault_dir.join("source.md"), content_with_ref).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Find the MystAnchor referenceable
    let referenceables = vault.select_referenceable_nodes(None);
    let anchor = referenceables
        .iter()
        .find(|r| matches!(r, Referenceable::MystAnchor(..)))
        .expect("Should find MystAnchor");

    // Find references to this anchor
    let refs = vault.select_references_for_referenceable(anchor);

    assert!(refs.is_some(), "Should find references to anchor");
    let refs = refs.unwrap();

    assert_eq!(refs.len(), 1, "Should find 1 reference to the anchor");

    // Verify it's a MystRole reference
    let (path, reference) = &refs[0];
    assert!(path.ends_with("source.md"), "Reference should be from source.md");

    match reference {
        Reference::MystRole(data, kind, target) => {
            assert_eq!(*kind, MystRoleKind::Ref);
            assert_eq!(target, "my-section");
            assert_eq!(data.reference_text, "my-section");
        }
        _ => panic!("Expected MystRole reference, got {:?}", reference),
    }
}
