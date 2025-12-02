//! Frontmatter JSON Schema Validation
//!
//! This module provides validation of YAML frontmatter against a JSON schema.
//! It is designed to integrate with the LSP diagnostics system to report
//! validation errors to users.
//!
//! ## Design Decisions
//!
//! - Schema is loaded once during vault construction and cached
//! - Validation produces diagnostics with Warning severity (not Error)
//! - Missing or malformed schema files result in graceful degradation
//! - YAML is converted to JSON Value for validation (jsonschema requirement)

use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;
use tower_lsp::lsp_types::{Position, Range};

/// Errors that can occur during frontmatter validation.
#[derive(Debug, Clone)]
pub struct FrontmatterValidationError {
    /// Human-readable error message from schema validation.
    pub message: String,
    /// The JSON path within the frontmatter where the error occurred.
    /// E.g., "/tags/0" for the first element of the tags array.
    pub instance_path: String,
}

/// Result of validating frontmatter against a schema.
#[derive(Debug)]
pub struct FrontmatterValidationResult {
    /// Validation errors found.
    pub errors: Vec<FrontmatterValidationError>,
    /// The range of the frontmatter block in the document.
    /// Used to position diagnostics.
    pub frontmatter_range: Option<Range>,
}

/// Loaded and compiled JSON schema for frontmatter validation.
/// This is cached in the Vault for reuse across files.
pub struct FrontmatterSchema {
    /// The compiled JSON schema validator.
    validator: jsonschema::Validator,
}

impl FrontmatterSchema {
    /// Load a JSON schema from a file path.
    ///
    /// Returns `None` if the file doesn't exist or can't be parsed.
    /// This enables graceful degradation - no schema means no validation.
    pub fn load(path: &Path) -> Option<Self> {
        // Read schema file
        let schema_content = std::fs::read_to_string(path).ok()?;

        // Parse JSON
        let schema_json: serde_json::Value = serde_json::from_str(&schema_content).ok()?;

        // Compile schema
        let validator = jsonschema::validator_for(&schema_json).ok()?;

        Some(FrontmatterSchema { validator })
    }

    /// Validate frontmatter text against the schema.
    ///
    /// # Arguments
    ///
    /// * `document_text` - The full document text (including frontmatter delimiters)
    ///
    /// # Returns
    ///
    /// A `FrontmatterValidationResult` containing any validation errors
    /// and the range of the frontmatter block.
    pub fn validate(&self, document_text: &str) -> FrontmatterValidationResult {
        // Extract frontmatter
        let Some((yaml_content, start_line, end_line)) =
            extract_frontmatter_with_range(document_text)
        else {
            // No frontmatter found - return empty result
            return FrontmatterValidationResult {
                errors: vec![],
                frontmatter_range: None,
            };
        };

        // Calculate the frontmatter range for diagnostics
        let frontmatter_range = Some(Range {
            start: Position {
                line: start_line as u32,
                character: 0,
            },
            end: Position {
                line: end_line as u32,
                character: 3, // Length of "---"
            },
        });

        // Parse YAML to JSON Value
        let yaml_value: serde_json::Value = match serde_yaml::from_str(&yaml_content) {
            Ok(v) => v,
            Err(e) => {
                // YAML parse error
                return FrontmatterValidationResult {
                    errors: vec![FrontmatterValidationError {
                        message: format!("YAML parse error: {}", e),
                        instance_path: String::new(),
                    }],
                    frontmatter_range,
                };
            }
        };

        // Validate against schema
        let errors: Vec<FrontmatterValidationError> = self
            .validator
            .iter_errors(&yaml_value)
            .map(|error| FrontmatterValidationError {
                message: error.to_string(),
                instance_path: error.instance_path.to_string(),
            })
            .collect();

        FrontmatterValidationResult {
            errors,
            frontmatter_range,
        }
    }
}

/// Regex to match frontmatter at the start of a document.
/// Captures the YAML content between the --- delimiters.
static FRONTMATTER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^---\n(?<yaml>(?:.|\n)*?)\n---").unwrap());

/// Extract the raw YAML content from frontmatter delimiters.
///
/// Returns the YAML content and its starting line number (0-indexed).
#[allow(dead_code)]
pub fn extract_frontmatter(text: &str) -> Option<(String, usize)> {
    let (yaml, start_line, _end_line) = extract_frontmatter_with_range(text)?;
    Some((yaml, start_line))
}

/// Extract frontmatter with full range information.
///
/// Returns (yaml_content, start_line, end_line) where:
/// - yaml_content: The YAML text between the delimiters
/// - start_line: Line number of the opening --- (0-indexed)
/// - end_line: Line number of the closing --- (0-indexed)
fn extract_frontmatter_with_range(text: &str) -> Option<(String, usize, usize)> {
    let captures = FRONTMATTER_RE.captures(text)?;
    let yaml_match = captures.name("yaml")?;
    let yaml_content = yaml_match.as_str().to_string();

    // The opening --- is at line 0
    let start_line = 0;

    // Calculate closing --- line:
    // Line 0: ---
    // Line 1..n: YAML content
    // Line n+1: ---
    // So end_line = number of lines in yaml + 1
    let end_line = yaml_content.lines().count() + 1;

    Some((yaml_content, start_line, end_line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ========================================================================
    // Test Scenario 1: Valid frontmatter passes validation - NO diagnostic
    // ========================================================================

    #[test]
    fn test_valid_frontmatter_no_errors() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.json");

        // Schema requiring title and tags
        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["title", "tags"],
            "properties": {
                "title": { "type": "string" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        }"#;

        fs::write(&schema_path, schema).unwrap();

        let validator =
            FrontmatterSchema::load(&schema_path).expect("Schema should load successfully");

        let document = r#"---
title: "My Document"
tags:
  - rust
  - testing
---
# Content here
"#;

        let result = validator.validate(document);
        assert!(
            result.errors.is_empty(),
            "Valid frontmatter should produce no errors, but got: {:?}",
            result.errors
        );
    }

    // ========================================================================
    // Test Scenario 2: Missing required field produces diagnostic
    // ========================================================================

    #[test]
    fn test_missing_required_field_produces_error() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.json");

        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["title", "author"],
            "properties": {
                "title": { "type": "string" },
                "author": { "type": "string" }
            }
        }"#;

        fs::write(&schema_path, schema).unwrap();

        let validator =
            FrontmatterSchema::load(&schema_path).expect("Schema should load successfully");

        // Missing 'author' field
        let document = r#"---
title: "My Document"
---
# Content here
"#;

        let result = validator.validate(document);
        assert_eq!(
            result.errors.len(),
            1,
            "Missing required field should produce 1 error"
        );
        assert!(
            result.errors[0].message.contains("author")
                || result.errors[0].message.contains("required"),
            "Error message should mention the missing field or 'required'"
        );
    }

    // ========================================================================
    // Test Scenario 3: Wrong type (string vs array) produces diagnostic
    // ========================================================================

    #[test]
    fn test_wrong_type_produces_error() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.json");

        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        }"#;

        fs::write(&schema_path, schema).unwrap();

        let validator =
            FrontmatterSchema::load(&schema_path).expect("Schema should load successfully");

        // tags is a string instead of an array
        let document = r#"---
tags: "not-an-array"
---
# Content
"#;

        let result = validator.validate(document);
        assert_eq!(result.errors.len(), 1, "Wrong type should produce 1 error");
        assert!(
            result.errors[0].message.contains("array") || result.errors[0].message.contains("type"),
            "Error message should mention expected type"
        );
        assert!(
            result.errors[0].instance_path.contains("tags"),
            "Instance path should point to 'tags' field"
        );
    }

    // ========================================================================
    // Test Scenario 4: Invalid enum value produces diagnostic
    // ========================================================================

    #[test]
    fn test_invalid_enum_value_produces_error() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.json");

        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["draft", "published", "archived"]
                }
            }
        }"#;

        fs::write(&schema_path, schema).unwrap();

        let validator =
            FrontmatterSchema::load(&schema_path).expect("Schema should load successfully");

        // Invalid enum value
        let document = r#"---
status: "invalid-status"
---
# Content
"#;

        let result = validator.validate(document);
        assert_eq!(
            result.errors.len(),
            1,
            "Invalid enum value should produce 1 error"
        );
        assert!(
            result.errors[0].instance_path.contains("status"),
            "Instance path should point to 'status' field"
        );
    }

    // ========================================================================
    // Test Scenario 5: Schema file not found - graceful degradation
    // ========================================================================

    #[test]
    fn test_schema_file_not_found_returns_none() {
        let nonexistent_path = Path::new("/nonexistent/path/schema.json");
        let result = FrontmatterSchema::load(nonexistent_path);
        assert!(
            result.is_none(),
            "Missing schema file should return None, not panic"
        );
    }

    // ========================================================================
    // Test Scenario 6: Malformed schema - graceful degradation
    // ========================================================================

    #[test]
    fn test_malformed_schema_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.json");

        // Invalid JSON
        fs::write(&schema_path, "{ not valid json }").unwrap();

        let result = FrontmatterSchema::load(&schema_path);
        assert!(
            result.is_none(),
            "Malformed schema should return None, not panic"
        );
    }

    // ========================================================================
    // Test Scenario 7: No frontmatter in file - no diagnostics
    // ========================================================================

    #[test]
    fn test_no_frontmatter_no_errors() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.json");

        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["title"]
        }"#;

        fs::write(&schema_path, schema).unwrap();

        let validator =
            FrontmatterSchema::load(&schema_path).expect("Schema should load successfully");

        // No frontmatter
        let document = "# Just a heading\n\nSome content.";

        let result = validator.validate(document);
        assert!(
            result.errors.is_empty(),
            "File without frontmatter should produce no errors"
        );
        assert!(
            result.frontmatter_range.is_none(),
            "File without frontmatter should have no range"
        );
    }

    // ========================================================================
    // Test Scenario 8: Malformed YAML frontmatter - diagnostic for parse error
    // ========================================================================

    #[test]
    fn test_malformed_yaml_produces_error() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.json");

        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        }"#;

        fs::write(&schema_path, schema).unwrap();

        let validator =
            FrontmatterSchema::load(&schema_path).expect("Schema should load successfully");

        // Malformed YAML (bad indentation)
        let document = r#"---
title: "Test"
  bad_indent: value
---
# Content
"#;

        let result = validator.validate(document);
        assert!(
            !result.errors.is_empty(),
            "Malformed YAML should produce at least one error"
        );
        assert!(
            result.errors[0].message.to_lowercase().contains("yaml")
                || result.errors[0].message.to_lowercase().contains("parse")
                || result.errors[0].message.to_lowercase().contains("invalid"),
            "Error should indicate YAML parsing failure"
        );
    }

    // ========================================================================
    // Additional: Test frontmatter range extraction
    // ========================================================================

    #[test]
    fn test_frontmatter_range_is_correct() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.json");

        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object"
        }"#;

        fs::write(&schema_path, schema).unwrap();

        let validator =
            FrontmatterSchema::load(&schema_path).expect("Schema should load successfully");

        let document = r#"---
title: "Test"
---
# Content
"#;

        let result = validator.validate(document);
        let range = result
            .frontmatter_range
            .expect("Should have frontmatter range");

        // Frontmatter starts at line 0 (the first ---)
        assert_eq!(range.start.line, 0, "Frontmatter should start at line 0");
        // Frontmatter ends at line 2 (the closing ---)
        assert_eq!(range.end.line, 2, "Frontmatter should end at line 2");
    }

    // ========================================================================
    // Test: Multiple validation errors
    // ========================================================================

    #[test]
    fn test_multiple_validation_errors() {
        let temp_dir = TempDir::new().unwrap();
        let schema_path = temp_dir.path().join("schema.json");

        let schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["title", "author"],
            "properties": {
                "title": { "type": "string" },
                "author": { "type": "string" },
                "count": { "type": "integer" }
            }
        }"#;

        fs::write(&schema_path, schema).unwrap();

        let validator =
            FrontmatterSchema::load(&schema_path).expect("Schema should load successfully");

        // Missing both required fields and wrong type for count
        let document = r#"---
count: "not-a-number"
---
# Content
"#;

        let result = validator.validate(document);
        assert!(
            result.errors.len() >= 2,
            "Should have at least 2 errors (missing required fields), got {}",
            result.errors.len()
        );
    }

    // ========================================================================
    // Test: extract_frontmatter helper
    // ========================================================================

    #[test]
    fn test_extract_frontmatter_basic() {
        let document = r#"---
title: Test
---
# Content"#;

        let (yaml, start_line) = extract_frontmatter(document).expect("Should extract frontmatter");

        assert_eq!(
            start_line, 0,
            "Frontmatter starts at line 0 (the opening ---)"
        );
        assert!(yaml.contains("title: Test"));
    }

    #[test]
    fn test_extract_frontmatter_no_frontmatter() {
        let document = "# Just content\n\nNo frontmatter here.";
        assert!(
            extract_frontmatter(document).is_none(),
            "Document without frontmatter should return None"
        );
    }

    #[test]
    fn test_extract_frontmatter_not_at_start() {
        // Frontmatter must be at the very start of the document
        let document = "\n---\ntitle: Test\n---\n# Content";
        assert!(
            extract_frontmatter(document).is_none(),
            "Frontmatter not at document start should return None"
        );
    }
}
