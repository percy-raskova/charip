use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Nested MyST configuration in frontmatter.
#[derive(Deserialize, Debug, Clone, Default)]
struct MystConfig {
    #[serde(default)]
    substitutions: HashMap<String, String>,
}

/// Raw frontmatter structure for parsing.
/// This intermediate struct captures both top-level and nested substitution definitions.
#[derive(Deserialize, Debug, Clone)]
struct RawFrontmatter {
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    substitutions: HashMap<String, String>,
    #[serde(default)]
    myst: Option<MystConfig>,
}

/// Parsed metadata from Markdown frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MDMetadata {
    aliases: Vec<String>,
    /// Substitution definitions merged from both `substitutions` and `myst.substitutions`.
    /// When both are present, `myst.substitutions` takes precedence for conflicting keys.
    substitutions: HashMap<String, String>,
}

impl Hash for MDMetadata {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.aliases.hash(state);
        // Sort substitutions by key for deterministic hashing
        let mut pairs: Vec<_> = self.substitutions.iter().collect();
        pairs.sort_by_key(|i| i.0);
        pairs.hash(state);
    }
}

impl MDMetadata {
    pub fn new(text: &str) -> Option<MDMetadata> {
        // find text between --- at the beginning of the file

        static RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"^---\n(?<metadata>(\n|.)*?)\n---").unwrap());

        let metadata_match = RE.captures_iter(text).next()?.name("metadata");

        let metadata_match = metadata_match?;

        let raw: RawFrontmatter = serde_yaml::from_str(metadata_match.as_str()).ok()?;

        // Merge substitutions: start with top-level, then override with myst.substitutions
        let mut substitutions = raw.substitutions;
        if let Some(myst_config) = raw.myst {
            // myst.substitutions takes precedence
            for (key, value) in myst_config.substitutions {
                substitutions.insert(key, value);
            }
        }

        Some(MDMetadata {
            aliases: raw.aliases,
            substitutions,
        })
    }

    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    /// Returns the substitution definitions from frontmatter.
    ///
    /// Substitutions are merged from both top-level `substitutions` key
    /// and nested `myst.substitutions` key. When both are present,
    /// `myst.substitutions` takes precedence for conflicting keys.
    pub fn substitutions(&self) -> &HashMap<String, String> {
        &self.substitutions
    }
}

#[cfg(test)]
mod tests {
    use crate::vault::metadata::MDMetadata;

    #[test]
    fn test_aliases() {
        let metadata = MDMetadata::new("---\naliases: [\"alias1\", \"alias2\"]\n---").unwrap();
        assert_eq!(metadata.aliases, vec!["alias1", "alias2"]);
    }

    #[test]
    fn test_alias_list() {
        let metadata = MDMetadata::new(
            r"---
aliases:
    - alias1
    - alias2
---",
        )
        .unwrap();
        assert_eq!(metadata.aliases(), &["alias1", "alias2"]);
    }

    // ========================================================================
    // Substitution Definition Tests (Chunk 9)
    // ========================================================================

    #[test]
    fn test_substitution_definitions_from_myst_key() {
        let text = r#"---
myst:
  substitutions:
    project: "MyProject"
    version: "1.0.0"
---
Content"#;
        let metadata = MDMetadata::new(text).unwrap();
        assert!(metadata.substitutions().contains_key("project"));
        assert!(metadata.substitutions().contains_key("version"));
        assert_eq!(
            metadata.substitutions().get("project"),
            Some(&"MyProject".to_string())
        );
    }

    #[test]
    fn test_substitution_definitions_from_top_level() {
        let text = r#"---
substitutions:
  name: "Test"
  count: "42"
---
Content"#;
        let metadata = MDMetadata::new(text).unwrap();
        assert!(metadata.substitutions().contains_key("name"));
        assert_eq!(
            metadata.substitutions().get("name"),
            Some(&"Test".to_string())
        );
    }

    #[test]
    fn test_substitution_myst_takes_precedence() {
        // When both myst.substitutions and substitutions are present,
        // myst.substitutions should take precedence
        let text = r#"---
substitutions:
  name: "TopLevel"
  only_top: "OnlyTop"
myst:
  substitutions:
    name: "MystLevel"
    only_myst: "OnlyMyst"
---
Content"#;
        let metadata = MDMetadata::new(text).unwrap();
        // myst.substitutions takes precedence for name
        assert_eq!(
            metadata.substitutions().get("name"),
            Some(&"MystLevel".to_string())
        );
        // Both sources contribute
        assert!(metadata.substitutions().contains_key("only_top"));
        assert!(metadata.substitutions().contains_key("only_myst"));
    }

    #[test]
    fn test_no_substitutions() {
        let text = r#"---
aliases: ["test"]
---
Content"#;
        let metadata = MDMetadata::new(text).unwrap();
        assert!(metadata.substitutions().is_empty());
    }

    #[test]
    fn test_empty_substitutions() {
        let text = r#"---
substitutions: {}
---
Content"#;
        let metadata = MDMetadata::new(text).unwrap();
        assert!(metadata.substitutions().is_empty());
    }

    #[test]
    fn test_substitutions_with_special_characters() {
        let text = r#"---
substitutions:
  greeting: "Hello, World!"
  math: "E = mc^2"
---
Content"#;
        let metadata = MDMetadata::new(text).unwrap();
        assert_eq!(
            metadata.substitutions().get("greeting"),
            Some(&"Hello, World!".to_string())
        );
        assert_eq!(
            metadata.substitutions().get("math"),
            Some(&"E = mc^2".to_string())
        );
    }
}
