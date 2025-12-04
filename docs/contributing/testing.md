---
title: Testing
---

# Testing

charip-lsp has comprehensive test coverage. This guide explains how to run and write tests.

## Running Tests

### All Tests

```bash
cargo test
```

### Specific Test

```bash
cargo test test_name
```

### Specific Module

```bash
cargo test vault::
cargo test completion::
```

### With Output

See println! output from passing tests:

```bash
cargo test -- --nocapture
```

## Test Organization

Tests are co-located with source code:

```rust
// src/vault/mod.rs

pub fn some_function() { ... }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_some_function() {
        // ...
    }
}
```

## Test Categories

### Unit Tests

Test individual functions in isolation:

```rust
#[test]
fn test_parse_anchor() {
    let result = parse_anchor("(my-anchor)=");
    assert_eq!(result.name, "my-anchor");
}
```

### Integration Tests

Test components working together:

```rust
#[test]
fn test_vault_indexes_anchors() {
    let vault = create_test_vault();
    let anchors = vault.select_myst_symbols(&path);
    assert!(!anchors.is_empty());
}
```

### Graph Tests

Test the petgraph-based vault:

```rust
#[test]
fn test_backlinks_via_graph() {
    let vault = create_vault_with_links();
    let backlinks = vault.incoming_references(&target_path);
    assert_eq!(backlinks.len(), 3);
}
```

## Test Utilities

### Creating Test Vaults

```rust
use crate::test_utils::create_test_vault_dir;

#[test]
fn test_with_vault() {
    let (_temp_dir, vault_dir) = create_test_vault_dir();

    // Create test files
    std::fs::write(vault_dir.join("test.md"), "# Test\n").unwrap();

    // Build vault
    let settings = Settings::default();
    let vault = Vault::construct_vault(&settings, &vault_dir).unwrap();

    // Test
    assert!(vault.get_document(&vault_dir.join("test.md")).is_some());
}
```

### Test Files Directory

`TestFiles/` contains a sample vault for manual testing:

```
TestFiles/
├── Test.md
├── daily/
│   └── 2024-03-17.md
├── folder/
│   └── nested.md
└── .moxide.toml
```

## Writing Good Tests

### Test Naming

Use descriptive names:

```rust
#[test]
fn test_ref_role_completes_anchors_from_all_files() { ... }

#[test]
fn test_diagnostic_reports_missing_image() { ... }
```

### Test Structure

Follow Arrange-Act-Assert:

```rust
#[test]
fn test_rename_updates_references() {
    // Arrange
    let vault = create_vault_with_anchor("old-name");

    // Act
    let edits = rename_anchor(&vault, "old-name", "new-name");

    // Assert
    assert_eq!(edits.len(), 3);
    assert!(edits[0].new_text.contains("new-name"));
}
```

### Test Edge Cases

```rust
#[test]
fn test_empty_vault() { ... }

#[test]
fn test_file_with_no_references() { ... }

#[test]
fn test_circular_include() { ... }
```

## Test Coverage

Current coverage by component:

| Component | Tests |
|-----------|-------|
| Vault/Graph | 180+ |
| Completions | 40+ |
| Diagnostics | 60+ |
| References | 30+ |
| Rename | 20+ |
| **Total** | **347** |

## Adding Tests for New Features

1. **Write failing tests first** (TDD red phase)
2. Implement the feature
3. Verify tests pass (green phase)
4. Refactor if needed

Example for a new diagnostic:

```rust
#[test]
fn test_diagnostic_for_new_error_type() {
    let vault = create_vault_with_error();
    let diagnostics = compute_diagnostics(&vault, &path);

    assert!(diagnostics.iter().any(|d|
        d.message.contains("expected error message")
    ));
}
```

## CI Requirements

All tests must pass in CI:

```bash
cargo test --verbose
```

PR checks:
1. Build succeeds
2. All tests pass
3. No formatting issues (`cargo fmt --check`)
