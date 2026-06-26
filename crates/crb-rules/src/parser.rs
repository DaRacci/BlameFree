//! YAML frontmatter parsing for rule files.
//!
//! Rule files are markdown (`.md`) with optional YAML frontmatter delimited by
//! `---` lines.  If no frontmatter is present the entire file is treated as an
//! always-apply rule body.

use std::path::Path;

use serde::Deserialize;

use crate::Rule;

// ── Frontmatter Types ────────────────────────────────────────────────────

/// Intermediate deserialization target for YAML frontmatter.
///
/// All fields are optional because the frontmatter may omit any of them; the
/// parser fills in sensible defaults.
#[derive(Debug, Clone, Deserialize)]
pub struct RuleMetadata {
    /// Human-readable description.
    pub description: Option<String>,
    /// Glob patterns (single string or array).  Defaults to empty.
    #[serde(default)]
    pub globs: Option<GlobsField>,
    /// If `true`, the rule always applies.  Defaults to `false` when a
    /// frontmatter block exists, but when there is *no* frontmatter the rule
    /// is treated as always-apply (see [`parse_rule_file`]).
    #[serde(default)]
    pub always_apply: Option<bool>,
}

/// Accept a single string or an array of strings for the `globs` field.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum GlobsField {
    /// `globs: "**/*.py"`
    Single(String),
    /// `globs: ["**/*.py", "**/*.pyi"]`
    Multiple(Vec<String>),
}

// ── Parser ───────────────────────────────────────────────────────────────

/// Parse a rule from a markdown string with optional YAML frontmatter.
///
/// # Frontmatter format
///
/// ```markdown
/// ---
/// description: My Rule
/// globs: "**/*.py"
/// always_apply: false
/// ---
/// Rule body content here...
/// ```
///
/// # No frontmatter
///
/// If the content does not start with `---`, the entire content is treated as
/// the rule body and the rule is set to `always_apply: true`.
///
/// # Errors
///
/// Returns an error if:
/// - The content starts with `---` but has fewer than two `---` separators
///   (malformed frontmatter).
/// - The YAML between the `---` delimiters cannot be parsed.
pub fn parse_rule_file(content: &str, source_file: &Path) -> anyhow::Result<Rule> {
    if !content.starts_with("---") {
        // No frontmatter — treat as always-apply rule with empty metadata.
        return Ok(Rule {
            description: None,
            globs: vec![],
            always_apply: true,
            body: content.trim().to_string(),
            source_file: source_file.to_path_buf(),
        });
    }

    // Split on "---", max 3 parts: [empty, frontmatter, body]
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        anyhow::bail!(
            "Malformed frontmatter in {}: expected --- blocks, got {} part(s)",
            source_file.display(),
            parts.len()
        );
    }

    let yaml_str = parts[1];
    let body = parts[2].trim().to_string();
    let metadata: RuleMetadata = serde_yaml::from_str(yaml_str)?;

    // Convert GlobsField to Vec<String>
    let globs = match metadata.globs {
        Some(GlobsField::Single(s)) => vec![s],
        Some(GlobsField::Multiple(v)) => v,
        None => vec![],
    };

    Ok(Rule {
        description: metadata.description,
        globs,
        always_apply: metadata.always_apply.unwrap_or(false),
        body,
        source_file: source_file.to_path_buf(),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn source(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    #[test]
    fn test_valid_frontmatter_single_glob() {
        let content = "---\ndescription: My Rule\nglobs: \"**/*.py\"\n---\nUse type hints.";
        let rule = parse_rule_file(content, &source("test.md")).unwrap();
        assert_eq!(rule.description.as_deref(), Some("My Rule"));
        assert_eq!(rule.globs, vec!["**/*.py"]);
        assert!(!rule.always_apply);
        assert_eq!(rule.body, "Use type hints.");
        assert_eq!(rule.source_file.to_string_lossy(), "test.md");
    }

    #[test]
    fn test_valid_frontmatter_multiple_globs() {
        let content = "---\ndescription: TS Rule\nglobs:\n  - \"**/*.ts\"\n  - \"**/*.tsx\"\n---\nTS body.";
        let rule = parse_rule_file(content, &source("ts.md")).unwrap();
        assert_eq!(rule.description.as_deref(), Some("TS Rule"));
        assert_eq!(rule.globs, vec!["**/*.ts", "**/*.tsx"]);
        assert!(!rule.always_apply);
        assert_eq!(rule.body, "TS body.");
    }

    #[test]
    fn test_no_frontmatter_defaults_to_always_apply() {
        let content = "Some plain markdown content.\n\nWith multiple lines.";
        let rule = parse_rule_file(content, &source("plain.md")).unwrap();
        assert!(rule.description.is_none());
        assert!(rule.globs.is_empty());
        assert!(rule.always_apply);
        assert_eq!(rule.body, "Some plain markdown content.\n\nWith multiple lines.");
    }

    #[test]
    fn test_malformed_frontmatter_single_delimiter() {
        let content = "---\nOnly one delimiter, no closing.";
        let result = parse_rule_file(content, &source("bad.md"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Malformed frontmatter") || err.contains("expected ---"));
    }

    #[test]
    fn test_invalid_yaml_in_frontmatter() {
        let content = "---\ninvalid: [yaml: broken\n---\nBody.";
        let result = parse_rule_file(content, &source("bad.md"));
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_optional_fields() {
        let content = "---\nglobs: \"**/*.rs\"\n---\nRust body.";
        let rule = parse_rule_file(content, &source("rust.md")).unwrap();
        // description should be None, always_apply defaults to false
        assert!(rule.description.is_none());
        assert_eq!(rule.globs, vec!["**/*.rs"]);
        assert!(!rule.always_apply);
        assert_eq!(rule.body, "Rust body.");
    }

    #[test]
    fn test_always_apply_true_from_frontmatter() {
        let content = "---\ndescription: Security\nalways_apply: true\n---\nCheck for OWASP top 10.";
        let rule = parse_rule_file(content, &source("security.md")).unwrap();
        assert!(rule.always_apply);
        assert_eq!(rule.body, "Check for OWASP top 10.");
    }

    #[test]
    fn test_frontmatter_with_extra_fields() {
        // serde_yaml ignores unknown fields by default, so extra fields are OK.
        let content = "---\ndescription: Extra\nglobs: \"*.py\"\npriority: high\ntags: [lint]\n---\nBody.";
        let rule = parse_rule_file(content, &source("extra.md")).unwrap();
        assert_eq!(rule.description.as_deref(), Some("Extra"));
        assert_eq!(rule.globs, vec!["*.py"]);
        assert_eq!(rule.body, "Body.");
    }

    #[test]
    fn test_globs_field_single_vs_multiple() {
        let single = "---\nglobs: \"*.py\"\n---\nBody.";
        let multi = "---\nglobs:\n  - \"*.py\"\n  - \"*.pyi\"\n---\nBody.";

        let rule_single = parse_rule_file(single, &source("s.md")).unwrap();
        let rule_multi = parse_rule_file(multi, &source("m.md")).unwrap();

        assert_eq!(rule_single.globs, vec!["*.py"]);
        assert_eq!(rule_multi.globs, vec!["*.py", "*.pyi"]);
    }

    #[test]
    fn test_empty_body_after_frontmatter() {
        let content = "---\ndescription: Empty\nglobs: \"*.txt\"\n---\n";
        let rule = parse_rule_file(content, &source("empty.md")).unwrap();
        assert_eq!(rule.body, "");
    }
}
