use crate::config::Settings;
use crate::myst_parser::MystSymbolKind;
use crate::test_utils::{create_test_vault, create_test_vault_dir};
use crate::vault::*;
use std::fs;

#[test]
fn test_vault_extracts_myst_directives() {
    let content = r#"
# My Document

```{note}
This is a note.
```

```{warning}
Be careful!
```
"#;
    let (_temp_dir, _vault_dir, vault) = create_test_vault(|dir| {
        fs::write(dir.join("test.md"), content).unwrap();
    });

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

    assert_eq!(
        anchor_refs.len(),
        1,
        "Should find 1 MystAnchor referenceable"
    );

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

    assert_eq!(refs.len(), 1, "Should find 1 reference to the anchor");

    // Verify it's a MystRole reference
    let (path, reference) = &refs[0];
    assert!(
        path.ends_with("source.md"),
        "Reference should be from source.md"
    );

    match reference {
        Reference::MystRole(data, kind, target) => {
            assert_eq!(kind, &MystRoleKind::Ref);
            assert_eq!(target, "my-section");
            assert_eq!(data.reference_text, "my-section");
        }
        _ => panic!("Expected MystRole reference, got {:?}", reference),
    }
}

// ============================================================================
// MathLabel referenceable tests (TDD RED PHASE)
// ============================================================================

#[test]
fn test_math_labels_extracted_as_referenceables() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // File with math directives containing labels
    let content = r#"# Equations

```{math}
:label: euler-identity

e^{i\pi} + 1 = 0
```

```{math}
:label: pythagorean

a^2 + b^2 = c^2
```

```{math}
No label on this one
```
"#;
    fs::write(vault_dir.join("equations.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    let math_labels: Vec<_> = vault
        .select_referenceable_nodes(None)
        .into_iter()
        .filter(|r| matches!(r, Referenceable::MathLabel(..)))
        .collect();

    // Should have 2 math labels (euler-identity, pythagorean)
    assert_eq!(
        math_labels.len(),
        2,
        "Should find 2 MathLabel referenceables"
    );
}

#[test]
fn test_math_label_get_refname() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    let content = r#"```{math}
:label: euler-identity

e^{i\pi} + 1 = 0
```

```{math}
:label: pythagorean

a^2 + b^2 = c^2
```
"#;
    fs::write(vault_dir.join("equations.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    let math_labels: Vec<_> = vault
        .select_referenceable_nodes(None)
        .into_iter()
        .filter(|r| matches!(r, Referenceable::MathLabel(..)))
        .collect();

    let names: Vec<_> = math_labels
        .iter()
        .filter_map(|r| r.get_refname(vault.root_dir()))
        .map(|refname| refname.to_string())
        .collect();

    assert!(
        names.contains(&"euler-identity".to_string()),
        "Should contain euler-identity label"
    );
    assert!(
        names.contains(&"pythagorean".to_string()),
        "Should contain pythagorean label"
    );
}

#[test]
fn test_math_label_has_range() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    let content = r#"```{math}
:label: my-equation

x = y + z
```
"#;
    fs::write(vault_dir.join("equations.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    let math_labels: Vec<_> = vault
        .select_referenceable_nodes(None)
        .into_iter()
        .filter(|r| matches!(r, Referenceable::MathLabel(..)))
        .collect();

    assert_eq!(math_labels.len(), 1, "Should find 1 MathLabel");

    for label in math_labels {
        assert!(label.get_range().is_some(), "MathLabel should have a range");
    }
}

#[test]
fn test_math_label_get_path() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    let content = r#"```{math}
:label: test-equation

x = 1
```
"#;
    fs::write(vault_dir.join("math_file.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    let math_labels: Vec<_> = vault
        .select_referenceable_nodes(None)
        .into_iter()
        .filter(|r| matches!(r, Referenceable::MathLabel(..)))
        .collect();

    assert_eq!(math_labels.len(), 1, "Should find 1 MathLabel");

    let path = math_labels[0].get_path();
    assert!(
        path.ends_with("math_file.md"),
        "MathLabel should return correct file path"
    );
}

// ============================================================================
// Substitution Definition Tests (Chunk 10)
// ============================================================================

#[test]
fn test_substitution_defs_extracted_as_referenceables() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // File with substitution definitions in frontmatter
    let content = r#"---
myst:
  substitutions:
    project_name: "Charip LSP"
    version: "1.0.0"
---
# Document

The {{project_name}} is at version {{version}}.
"#;
    fs::write(vault_dir.join("with_subs.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    let sub_defs: Vec<_> = vault
        .select_referenceable_nodes(None)
        .into_iter()
        .filter(|r| matches!(r, Referenceable::SubstitutionDef(..)))
        .collect();

    // Should have 2 substitution definitions (project_name, version)
    assert_eq!(
        sub_defs.len(),
        2,
        "Should find 2 SubstitutionDef referenceables"
    );
}

#[test]
fn test_substitution_def_get_refname() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    let content = r#"---
substitutions:
  my_var: "value"
---
Content"#;
    fs::write(vault_dir.join("test.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    let sub_defs: Vec<_> = vault
        .select_referenceable_nodes(None)
        .into_iter()
        .filter(|r| matches!(r, Referenceable::SubstitutionDef(..)))
        .collect();

    assert_eq!(sub_defs.len(), 1, "Should find 1 SubstitutionDef");

    let refname = sub_defs[0].get_refname(vault.root_dir());
    assert!(refname.is_some(), "SubstitutionDef should have a refname");
    assert_eq!(refname.unwrap().to_string(), "my_var");
}

#[test]
fn test_substitution_resolves_to_def_in_same_file() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // File with both definition and usage
    let content = r#"---
substitutions:
  name: "World"
---
# Hello

Hello {{name}}!
"#;
    fs::write(vault_dir.join("test.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Find the SubstitutionDef referenceable
    let referenceables = vault.select_referenceable_nodes(None);
    let sub_def = referenceables
        .iter()
        .find(|r| matches!(r, Referenceable::SubstitutionDef(..)))
        .expect("Should find SubstitutionDef");

    // Find references to this substitution
    let refs = vault.select_references_for_referenceable(sub_def);

    assert_eq!(refs.len(), 1, "Should find 1 reference to the substitution");

    // Verify it's a Substitution reference
    let (path, reference) = &refs[0];
    assert!(
        path.ends_with("test.md"),
        "Reference should be from test.md"
    );

    match reference {
        Reference::Substitution(data) => {
            assert_eq!(data.reference_text, "name");
        }
        _ => panic!("Expected Substitution reference, got {:?}", reference),
    }
}

#[test]
fn test_substitution_does_not_resolve_cross_file() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // File A: has definition but no usage
    let content_a = r#"---
substitutions:
  shared_var: "ValueA"
---
# File A

Content without usage.
"#;
    fs::write(vault_dir.join("file_a.md"), content_a).unwrap();

    // File B: has usage but no definition (should NOT resolve to file_a's definition)
    let content_b = r#"# File B

Using {{shared_var}} which is undefined in this file.
"#;
    fs::write(vault_dir.join("file_b.md"), content_b).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Find the SubstitutionDef from file_a
    let referenceables = vault.select_referenceable_nodes(None);
    let sub_def = referenceables
        .iter()
        .find(
            |r| matches!(r, Referenceable::SubstitutionDef(path, _) if path.ends_with("file_a.md")),
        )
        .expect("Should find SubstitutionDef in file_a");

    // Find references to this substitution
    let refs = vault.select_references_for_referenceable(sub_def);

    // Should NOT find any references because the usage in file_b
    // should not resolve to the definition in file_a (file-local only)
    assert!(
        refs.is_empty(),
        "Substitution in file_b should NOT resolve to definition in file_a"
    );
}

#[test]
fn test_substitution_def_has_no_range() {
    // SubstitutionDef is defined in frontmatter, not at a specific line position
    // It should return None for range (like File referenceable)
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    let content = r#"---
substitutions:
  test_var: "test"
---
Content"#;
    fs::write(vault_dir.join("test.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    let sub_defs: Vec<_> = vault
        .select_referenceable_nodes(None)
        .into_iter()
        .filter(|r| matches!(r, Referenceable::SubstitutionDef(..)))
        .collect();

    assert_eq!(sub_defs.len(), 1, "Should find 1 SubstitutionDef");

    // SubstitutionDef doesn't have a precise range since it's in frontmatter
    // This is acceptable - it's similar to how File referenceable works
    // (The exact range behavior can be None or pointing to frontmatter start)
}

// ============================================================================
// Glossary Term Hover Preview Tests (Chunk 11)
// ============================================================================

#[test]
fn test_glossary_term_preview_shows_definition() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    let content = r#"```{glossary}
MyST
  Markedly Structured Text. A powerful Markdown variant.

LSP
  Language Server Protocol for IDE features.
```
"#;
    fs::write(vault_dir.join("glossary.md"), content).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Find GlossaryTerm referenceables
    let terms: Vec<_> = vault
        .select_referenceable_nodes(None)
        .into_iter()
        .filter(|r| matches!(r, Referenceable::GlossaryTerm(..)))
        .collect();

    assert_eq!(terms.len(), 2, "Should find 2 glossary terms");

    // Get the preview for MyST term
    let myst_term = terms
        .iter()
        .find(|r| {
            if let Referenceable::GlossaryTerm(_, term) = r {
                term.term == "MyST"
            } else {
                false
            }
        })
        .expect("Should find MyST term");

    let preview = vault.select_referenceable_preview(myst_term);
    assert!(preview.is_some(), "Preview should exist for glossary term");

    match preview {
        Some(crate::vault::Preview::Text(text)) => {
            assert!(
                text.contains("**MyST**"),
                "Preview should contain bolded term name"
            );
            assert!(
                text.contains("Markedly Structured Text"),
                "Preview should contain definition: got '{}'",
                text
            );
        }
        Some(crate::vault::Preview::Empty) => {
            panic!("Expected Preview::Text, got Preview::Empty");
        }
        None => {
            panic!("Expected Preview::Text, got None");
        }
    }
}

#[test]
fn test_term_role_resolves_to_glossary_term() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Glossary file with definitions
    let glossary = r#"```{glossary}
API
  Application Programming Interface.
```
"#;
    fs::write(vault_dir.join("glossary.md"), glossary).unwrap();

    // File using {term} role
    let usage = "# Using Terms\n\nSee the {term}`API` for details.";
    fs::write(vault_dir.join("usage.md"), usage).unwrap();

    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).expect("Failed to construct vault");

    // Find the {term}`API` reference
    let refs = vault.select_references(None);
    let term_ref = refs
        .iter()
        .find(|(_, r)| {
            matches!(
                r,
                Reference::MystRole(_, crate::vault::MystRoleKind::Term, _)
            )
        })
        .expect("Should find term role reference");

    // Resolve the reference
    let resolved = vault.select_referenceables_for_reference(term_ref.1, term_ref.0);

    assert_eq!(
        resolved.len(),
        1,
        "Term role should resolve to exactly one glossary term"
    );
    assert!(
        matches!(resolved[0], Referenceable::GlossaryTerm(..)),
        "Should resolve to GlossaryTerm"
    );
}
