//! Rule loading and matching for the code review benchmark harness.
//!
//! `crb-rules` implements a rule system loosely inspired by Cursor, Continue,
//! and Cline: markdown files with YAML frontmatter, directory-based discovery
//! under `.riv/rules/`, `always_apply` and `globs` fields for matching.
//!
//! The crate exposes [`RuleSet`] as its primary API: load rules from a
//! directory, match them against changed file paths, and format an optional
//! preamble for injection into agent system prompts.

pub mod matcher;
pub mod parser;
pub mod preamble;

use std::{
    fs,
    path::{Path, PathBuf},
};

use tracing::{info, warn};

pub const RULES_DIR: &str = ".riv/rules";

/// A single rule loaded from a `.md` file with optional YAML frontmatter.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Human-readable description of the rule.
    pub description: Option<String>,

    /// Glob patterns that determine which file paths this rule applies to.
    pub globs: Vec<String>,

    /// If `true`, this rule always applies regardless of file paths.
    pub always_apply: bool,

    /// The content after the YAML frontmatter,
    /// or the whole file when no frontmatter is present.
    pub body: String,

    /// Origin file path from which this rule was loaded.
    pub source_file: PathBuf,
}

/// A loaded ruleset, cached from a directory of `.md` rule files.
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    /// All non-always-apply rules.
    pub rules: Vec<Rule>,

    /// Rules with `always_apply == true`, cached at load time so that
    /// [`RuleSet::matching`] avoids re-filtering on every call.
    pub always_rules: Vec<Rule>,
}

impl RuleSet {
    /// Load all `.md` rule files from `dir`.
    ///
    /// If `dir` does not exist, returns an empty [`RuleSet`] (no error) so that
    /// the harness works without any rules configured.
    ///
    /// Each `.md` file is parsed via [`parse_rule_file`]; parse or I/O errors
    /// are logged via `tracing::warn!` and the offending file is skipped, so one
    /// bad file does not prevent loading the rest.
    #[allow(clippy::cognitive_complexity)]
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        if !dir.exists() || !dir.is_dir() {
            info!("Rules directory does not exist: {}", dir.display());
            return Ok(RuleSet::default());
        }

        let mut rules = Vec::new();
        let mut bad_files = 0;

        let readdir = fs::read_dir(dir)?;
        for entry in readdir {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Error reading directory entry in {}: {e}", dir.display());
                    continue;
                }
            };

            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "md") {
                continue;
            }

            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to read rule file {}: {e}", path.display());
                    bad_files += 1;
                    continue;
                }
            };

            match crate::parser::parse_rule_file(&content, &path) {
                Ok(rule) => rules.push(rule),
                Err(e) => {
                    warn!("Failed to parse rule file {}: {e}", path.display());
                    bad_files += 1;
                }
            }
        }

        info!(
            "Loaded {} rule(s) from {} ({} skipped due to errors)",
            rules.len(),
            dir.display(),
            bad_files,
        );

        let (always_rules, non_always_rules) = rules.into_iter().partition(|r| r.always_apply);

        Ok(RuleSet {
            rules: non_always_rules,
            always_rules,
        })
    }

    /// Return all rules that apply to the given `file_paths`.
    ///
    /// Returns:
    /// 1. Every rule in [`always_rules`] (always-apply rules).
    /// 2. Any rule from [`rules`] whose globs match at least one of the given
    ///    file paths.
    pub fn matching(&self, file_paths: &[PathBuf]) -> Vec<&Rule> {
        let mut matched: Vec<&Rule> = self.always_rules.iter().collect();
        for rule in &self.rules {
            if rule.always_apply {
                continue; // already included via always_rules
            }
            if file_paths
                .iter()
                .any(|p| matcher::rule_matches_path(rule, p))
            {
                matched.push(rule);
            }
        }
        matched
    }

    /// Build a formatted preamble string suitable for injection into an agent's
    /// system prompt.
    ///
    /// Delegates to [`format_preamble`] with the result of [`RuleSet::matching`].
    pub fn format_preamble(&self, file_paths: &[PathBuf]) -> String {
        let matched = self.matching(file_paths);
        preamble::format_preamble(&matched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a temporary directory with a single rule file and load the
    /// ruleset from it. Returns the `TempDir` (keeps it alive) and the
    /// loaded `RuleSet`.
    fn with_rule_file(
        name: &str,
        frontmatter_globs: &str,
        body: &str,
    ) -> (tempfile::TempDir, RuleSet) {
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "---\ndescription: {}\nglobs: \"{}\"\n---\n{}",
            name, frontmatter_globs, body
        );
        std::fs::write(dir.path().join(name), content).unwrap();
        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        (dir, rs)
    }

    /// Create a temporary directory with a single always-apply rule file and
    /// load the ruleset from it.
    fn with_always_rule(description: &str, body: &str) -> (tempfile::TempDir, RuleSet) {
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            "---\ndescription: {}\nalways_apply: true\n---\n{}",
            description, body
        );
        std::fs::write(dir.path().join("always.md"), content).unwrap();
        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        (dir, rs)
    }

    #[test]
    fn test_load_from_nonexistent_dir_returns_empty() {
        let dir = Path::new("/tmp/nonexistent-rules-dir-12345");
        let rs = RuleSet::load_from_dir(dir).unwrap();
        assert!(rs.rules.is_empty());
        assert!(rs.always_rules.is_empty());
    }

    #[test]
    fn test_load_from_empty_dir_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        assert!(rs.rules.is_empty());
        assert!(rs.always_rules.is_empty());
    }

    #[test]
    fn test_load_rules_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("python-standards.md"),
            "---\ndescription: Python Standards\nglobs: \"**/*.py\"\n---\nUse type hints.",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("security.md"),
            "---\ndescription: Security\nalways_apply: true\n---\nCheck for SQL injection.",
        )
        .unwrap();

        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        // rule2 is always-apply -> goes to always_rules, rule1 stays in rules
        assert_eq!(rs.rules.len(), 1);
        assert_eq!(rs.always_rules.len(), 1);
    }

    #[test]
    fn test_matching_includes_always_rules() {
        let (_dir, rs) = with_always_rule("Always", "Always-on rule.");
        let matched = rs.matching(&[PathBuf::from("src/main.rs")]);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].description.as_deref(), Some("Always"));
    }

    #[test]
    fn test_matching_globs() {
        let (_dir, rs) = with_rule_file("py.md", "**/*.py", "Python rule.");
        let matched = rs.matching(&[PathBuf::from("src/main.py"), PathBuf::from("README.md")]);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].description.as_deref(), Some("py.md"));
    }

    #[test]
    fn test_matching_multiple_globs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ts.md"),
            "---\ndescription: TS\nglobs:\n  - \"**/*.ts\"\n  - \"**/*.tsx\"\n---\nTS rule.",
        )
        .unwrap();
        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        let matched = rs.matching(&[PathBuf::from("src/components.tsx")]);
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn test_matching_no_match_returns_empty() {
        let (_dir, rs) = with_rule_file("py.md", "**/*.py", "Python rule.");
        let matched = rs.matching(&[PathBuf::from("main.rs")]);
        assert!(matched.is_empty());
    }

    #[test]
    fn test_format_preamble_empty_when_no_match() {
        let (_dir, rs) = with_rule_file("py.md", "**/*.py", "Python rule.");
        let preamble = rs.format_preamble(&[PathBuf::from("main.rs")]);
        assert!(preamble.is_empty());
    }

    #[test]
    fn test_format_preamble_with_matched_rules() {
        let (_dir, rs) = with_always_rule("Always", "Always-on content.");
        let preamble = rs.format_preamble(&[PathBuf::from("any.py")]);
        assert!(!preamble.is_empty());
        assert!(preamble.contains("## Applicable Project Rules"));
        assert!(preamble.contains("### Always"));
        assert!(preamble.contains("Always-on content."));
    }

    #[test]
    fn test_load_from_dir_with_non_md_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "text file").unwrap();
        std::fs::write(dir.path().join("data.json"), r#"{"key": "value"}"#).unwrap();
        std::fs::write(dir.path().join("script.py"), "print('hello')").unwrap();
        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        insta::assert_debug_snapshot!(rs.rules.len(), @"0");
        insta::assert_debug_snapshot!(rs.always_rules.len(), @"0");
    }

    #[test]
    fn test_load_from_dir_with_parse_errors() {
        let dir = tempfile::tempdir().unwrap();
        // A valid rule file
        std::fs::write(
            dir.path().join("valid.md"),
            "---\ndescription: Valid\nglobs: \"**/*.py\"\n---\nValid rule.",
        )
        .unwrap();
        // Invalid frontmatter — single delimiter only
        std::fs::write(
            dir.path().join("invalid.md"),
            "---\nOnly one delimiter, no closing.",
        )
        .unwrap();
        // Invalid YAML — broken array
        std::fs::write(
            dir.path().join("bad-yaml.md"),
            "---\ninvalid: [yaml: broken\n---\nBody.",
        )
        .unwrap();
        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        // Only the valid file should be loaded
        insta::assert_debug_snapshot!(rs.rules.len(), @"1");
        insta::assert_debug_snapshot!(rs.always_rules.len(), @"0");
    }

    #[test]
    fn test_matching_empty_file_paths() {
        let (_dir, rs) = with_always_rule("Always", "Always-on.");
        let matched = rs.matching(&[]);
        // Always-rules are included even when no file paths are provided
        insta::assert_debug_snapshot!(matched.len(), @"1");
    }

    #[test]
    fn test_load_from_dir_called_on_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("not-a-dir.md");
        std::fs::write(&file_path, "---\ndescription: Test\n---\nBody.").unwrap();
        let rs = RuleSet::load_from_dir(&file_path).unwrap();
        // A plain file is not a directory, so returns empty ruleset
        insta::assert_debug_snapshot!(rs.rules.len(), @"0");
        insta::assert_debug_snapshot!(rs.always_rules.len(), @"0");
    }

    #[test]
    fn test_matching_with_both_always_and_globs_on_same_rule() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("both.md"),
            "---\ndescription: Both\nalways_apply: true\nglobs: \"**/*.py\"\n---\nBody.",
        )
        .unwrap();
        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        // Rule with always_apply: true goes to always_rules regardless of globs
        insta::assert_debug_snapshot!(rs.always_rules.len(), @"1");
        insta::assert_debug_snapshot!(rs.rules.len(), @"0");
        // Matching should include the rule exactly once (no double-count)
        let matched = rs.matching(&[PathBuf::from("test.py")]);
        insta::assert_debug_snapshot!(matched.len(), @"1");
    }
}
