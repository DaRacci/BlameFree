//! Rule loading and matching for the code review benchmark harness.
//!
//! `crb-rules` implements a rule system loosely inspired by Cursor, Continue,
//! and Cline: markdown files with YAML frontmatter, directory-based discovery
//! under `.crb/rules/`, `always_apply` and `globs` fields for matching.
//!
//! The crate exposes [`RuleSet`] as its primary API: load rules from a
//! directory, match them against changed file paths, and format an optional
//! preamble for injection into agent system prompts.

pub mod matcher;
pub mod parser;
pub mod preamble;

pub use matcher::{detect_language, detect_repo_languages};
pub use parser::parse_rule_file;
pub use preamble::format_preamble;

use std::path::{Path, PathBuf};

// ── Core Types ───────────────────────────────────────────────────────────

/// A single rule loaded from a `.md` file with optional YAML frontmatter.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Human-readable description of the rule (from frontmatter `description`).
    pub description: Option<String>,
    /// Glob patterns that determine which file paths this rule applies to.
    pub globs: Vec<String>,
    /// If `true`, this rule always applies regardless of file paths.
    pub always_apply: bool,
    /// Markdown body — the content after the YAML frontmatter (or the whole file
    /// when no frontmatter is present).
    pub body: String,
    /// Origin file path from which this rule was loaded.
    pub source_file: PathBuf,
}

/// A loaded ruleset, cached from a directory of `.md` rule files.
#[derive(Debug, Clone)]
pub struct RuleSet {
    /// All non-always-apply rules (those with globs that must be matched).
    pub rules: Vec<Rule>,
    /// Rules with `always_apply == true`, cached at load time so that
    /// [`RuleSet::matching`] avoids re-filtering on every call.
    pub always_rules: Vec<Rule>,
    /// The directory from which the ruleset was loaded.
    pub source_dir: PathBuf,
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
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        if !dir.exists() || !dir.is_dir() {
            tracing::info!(
                "Rules directory does not exist: {} — returning empty ruleset",
                dir.display()
            );
            return Ok(RuleSet {
                rules: Vec::new(),
                always_rules: Vec::new(),
                source_dir: dir.to_path_buf(),
            });
        }

        let mut rules: Vec<Rule> = Vec::new();
        let mut bad_files: usize = 0;

        let readdir = std::fs::read_dir(dir)?;
        for entry in readdir {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Error reading directory entry in {}: {e}", dir.display());
                    continue;
                }
            };

            let path = entry.path();
            if path.extension().map_or(true, |ext| ext != "md") {
                continue;
            }

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Failed to read rule file {}: {e}", path.display());
                    bad_files += 1;
                    continue;
                }
            };

            match parse_rule_file(&content, &path) {
                Ok(rule) => rules.push(rule),
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse rule file {}: {e}",
                        path.display()
                    );
                    bad_files += 1;
                }
            }
        }

        tracing::info!(
            "Loaded {} rule(s) from {} ({} skipped due to errors)",
            rules.len(),
            dir.display(),
            bad_files,
        );

        // Partition: always-apply rules go into the cached subset.
        let (always_rules, non_always_rules): (Vec<Rule>, Vec<Rule>) =
            rules.into_iter().partition(|r| r.always_apply);

        Ok(RuleSet {
            rules: non_always_rules,
            always_rules,
            source_dir: dir.to_path_buf(),
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

    /// Return rules whose globs match at least one file of the given language.
    ///
    /// This is a convenience wrapper for cases where you know the language but
    /// don't have concrete file paths.
    pub fn matching_language(&self, language: &str) -> Vec<&Rule> {
        // Check each rule's globs against a representative path pattern for the
        // language.  We use a synthetic path like `file.<ext>` to see if the
        // glob would match.
        let ext = language_to_extension(language);
        let synthetic_path = PathBuf::from(format!("file.{}", ext));

        let mut matched: Vec<&Rule> = self.always_rules.iter().collect();
        for rule in &self.rules {
            if rule.always_apply {
                continue;
            }
            if matcher::rule_matches_path(rule, &synthetic_path) {
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

/// Map a language identifier to a representative file extension.
///
/// Used by [`RuleSet::matching_language`] to create synthetic paths for glob
/// matching.
fn language_to_extension(language: &str) -> &'static str {
    match language {
        "python" => "py",
        "rust" => "rs",
        "typescript" => "ts",
        "javascript" => "js",
        "go" => "go",
        "ruby" => "rb",
        "java" => "java",
        "kotlin" => "kt",
        "swift" => "swift",
        "csharp" => "cs",
        "cpp" => "cpp",
        "c" => "c",
        "php" => "php",
        "scala" => "scala",
        _ => "txt",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        let rule1_path = dir.path().join("python-standards.md");
        let rule2_path = dir.path().join("security.md");

        std::fs::write(
            &rule1_path,
            "---\ndescription: Python Standards\nglobs: \"**/*.py\"\n---\nUse type hints.",
        )
        .unwrap();

        std::fs::write(
            &rule2_path,
            "---\ndescription: Security\nalways_apply: true\n---\nCheck for SQL injection.",
        )
        .unwrap();

        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        // rule2 is always-apply → goes to always_rules, rule1 stays in rules
        assert_eq!(rs.rules.len(), 1);
        assert_eq!(rs.always_rules.len(), 1);
    }

    #[test]
    fn test_matching_includes_always_rules() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("always.md"),
            "---\ndescription: Always\nalways_apply: true\n---\nAlways-on rule.",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("py.md"),
            "---\ndescription: Py\nglobs: \"**/*.py\"\n---\nPython rule.",
        )
        .unwrap();

        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        let matched = rs.matching(&[PathBuf::from("src/main.rs")]);
        // Always-apply rule is included even though .rs doesn't match py.md
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].description.as_deref(), Some("Always"));
    }

    #[test]
    fn test_matching_globs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("py.md"),
            "---\ndescription: Py\nglobs: \"**/*.py\"\n---\nPython rule.",
        )
        .unwrap();

        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        let matched =
            rs.matching(&[PathBuf::from("src/main.py"), PathBuf::from("README.md")]);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].description.as_deref(), Some("Py"));
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
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("py.md"),
            "---\ndescription: Py\nglobs: \"**/*.py\"\n---\nPython rule.",
        )
        .unwrap();

        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        let matched = rs.matching(&[PathBuf::from("main.rs")]);
        assert!(matched.is_empty());
    }

    #[test]
    fn test_matching_language() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("py.md"),
            "---\ndescription: Py\nglobs: \"**/*.py\"\n---\nPython rule.",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("rs.md"),
            "---\ndescription: Rs\nglobs: \"**/*.rs\"\n---\nRust rule.",
        )
        .unwrap();

        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        let matched = rs.matching_language("python");
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].description.as_deref(), Some("Py"));
    }

    #[test]
    fn test_format_preamble_empty_when_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("py.md"),
            "---\ndescription: Py\nglobs: \"**/*.py\"\n---\nPython rule.",
        )
        .unwrap();

        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        let preamble = rs.format_preamble(&[PathBuf::from("main.rs")]);
        assert!(preamble.is_empty());
    }

    #[test]
    fn test_format_preamble_with_matched_rules() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("always.md"),
            "---\ndescription: Always\nalways_apply: true\n---\nAlways-on content.",
        )
        .unwrap();

        let rs = RuleSet::load_from_dir(dir.path()).unwrap();
        let preamble = rs.format_preamble(&[PathBuf::from("any.py")]);
        assert!(!preamble.is_empty());
        assert!(preamble.contains("## Applicable Project Rules"));
        assert!(preamble.contains("### Always"));
        assert!(preamble.contains("Always-on content."));
    }
}
