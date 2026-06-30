//! Glob matching and language detection for rule files.
//!
//! Provides:
//! - [`rule_matches_path`] - check if a rule's glob patterns match a file path.
//! - [`detect_language`] - map a file extension to a language identifier.
//! - [`detect_repo_languages`] - collect unique languages from many paths.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::Rule;

// ── Glob Matching ────────────────────────────────────────────────────────

/// Check whether any of `rule`'s glob patterns match `path`.
///
/// Returns `false` immediately if `rule.globs` is empty (empty globs means
/// "no file-path match" - always-apply rules are handled separately by
/// [`RuleSet::matching`]).
pub fn rule_matches_path(rule: &Rule, path: &Path) -> bool {
    if rule.globs.is_empty() {
        return false;
    }
    let path_str = path.to_string_lossy();
    rule.globs.iter().any(|g| {
        glob::Pattern::new(g)
            .map(|p| p.matches(&path_str))
            .unwrap_or(false)
    })
}

// ── Language Detection ───────────────────────────────────────────────────

/// Map a file path to a language identifier based on its extension.
///
/// Returns `Some(language)` for known extensions, `None` for unknown ones.
pub fn detect_language(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "py" => Some("python"),
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" => Some("javascript"),
        "go" => Some("go"),
        "rb" => Some("ruby"),
        "java" => Some("java"),
        "kt" | "kts" => Some("kotlin"),
        "swift" => Some("swift"),
        "cs" => Some("csharp"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
        "c" | "h" => Some("c"),
        "scala" => Some("scala"),
        "php" => Some("php"),
        _ => None,
    }
}

/// Collect all unique language identifiers from a slice of file paths.
///
/// Internally calls [`detect_language`] on each path and collects non-`None`
/// results into a [`HashSet`].
pub fn detect_repo_languages(files: &[PathBuf]) -> HashSet<String> {
    files
        .iter()
        .filter_map(|f| detect_language(f))
        .map(String::from)
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn rule(globs: Vec<&str>) -> Rule {
        Rule {
            description: None,
            globs: globs.into_iter().map(String::from).collect(),
            always_apply: false,
            body: String::new(),
            source_file: PathBuf::from("test.md"),
        }
    }

    // ── rule_matches_path ────────────────────────────────────────────

    #[test]
    fn test_exact_glob_match() {
        let r = rule(vec!["src/main.py"]);
        assert!(rule_matches_path(&r, &PathBuf::from("src/main.py")));
    }

    #[test]
    fn test_wildcard_glob_match() {
        let r = rule(vec!["**/*.py"]);
        assert!(rule_matches_path(&r, &PathBuf::from("src/main.py")));
        assert!(rule_matches_path(&r, &PathBuf::from("tests/test_main.py")));
        assert!(!rule_matches_path(&r, &PathBuf::from("src/main.rs")));
    }

    #[test]
    fn test_nested_directory_glob_match() {
        let r = rule(vec!["src/**/*.rs"]);
        assert!(rule_matches_path(&r, &PathBuf::from("src/main.rs")));
        assert!(rule_matches_path(&r, &PathBuf::from("src/cli/args.rs")));
        assert!(!rule_matches_path(&r, &PathBuf::from("tests/main.rs")));
    }

    #[test]
    fn test_multiple_globs_any_match() {
        let r = rule(vec!["**/*.ts", "**/*.tsx"]);
        assert!(rule_matches_path(&r, &PathBuf::from("src/app.ts")));
        assert!(rule_matches_path(&r, &PathBuf::from("src/app.tsx")));
        assert!(!rule_matches_path(&r, &PathBuf::from("src/app.js")));
    }

    #[test]
    fn test_no_match_returns_false() {
        let r = rule(vec!["**/*.py"]);
        assert!(!rule_matches_path(&r, &PathBuf::from("main.rs")));
    }

    #[test]
    fn test_empty_globs_never_match() {
        let r = rule(vec![]);
        assert!(!rule_matches_path(&r, &PathBuf::from("anything.py")));
    }

    // ── detect_language ──────────────────────────────────────────────

    #[test]
    fn test_detect_python() {
        assert_eq!(detect_language(&PathBuf::from("main.py")), Some("python"));
    }

    #[test]
    fn test_detect_rust() {
        assert_eq!(detect_language(&PathBuf::from("lib.rs")), Some("rust"));
    }

    #[test]
    fn test_detect_typescript() {
        assert_eq!(
            detect_language(&PathBuf::from("app.ts")),
            Some("typescript")
        );
        assert_eq!(
            detect_language(&PathBuf::from("app.tsx")),
            Some("typescript")
        );
    }

    #[test]
    fn test_detect_javascript() {
        assert_eq!(
            detect_language(&PathBuf::from("app.js")),
            Some("javascript")
        );
        assert_eq!(
            detect_language(&PathBuf::from("app.jsx")),
            Some("javascript")
        );
    }

    #[test]
    fn test_detect_go() {
        assert_eq!(detect_language(&PathBuf::from("main.go")), Some("go"));
    }

    #[test]
    fn test_detect_cpp() {
        assert_eq!(detect_language(&PathBuf::from("main.cpp")), Some("cpp"));
        assert_eq!(detect_language(&PathBuf::from("main.cc")), Some("cpp"));
        assert_eq!(detect_language(&PathBuf::from("main.hpp")), Some("cpp"));
    }

    #[test]
    fn test_detect_c() {
        assert_eq!(detect_language(&PathBuf::from("main.c")), Some("c"));
        assert_eq!(detect_language(&PathBuf::from("main.h")), Some("c"));
    }

    #[test]
    fn test_detect_unknown_extension() {
        assert_eq!(detect_language(&PathBuf::from("Makefile")), None);
        assert_eq!(detect_language(&PathBuf::from("Dockerfile")), None);
        assert_eq!(detect_language(&PathBuf::from("file.txt")), None);
    }

    #[test]
    fn test_detect_no_extension() {
        assert_eq!(detect_language(&PathBuf::from("README")), None);
    }

    // ── detect_repo_languages ────────────────────────────────────────

    #[test]
    fn test_repo_languages_from_multiple_files() {
        let files = vec![
            PathBuf::from("src/main.py"),
            PathBuf::from("src/lib.rs"),
            PathBuf::from("src/app.ts"),
            PathBuf::from("README.md"),
        ];
        let langs = detect_repo_languages(&files);
        let mut expected: HashSet<String> = HashSet::new();
        expected.insert("python".to_string());
        expected.insert("rust".to_string());
        expected.insert("typescript".to_string());
        assert_eq!(langs, expected);
    }

    #[test]
    fn test_repo_languages_empty() {
        assert!(detect_repo_languages(&[]).is_empty());
    }

    #[test]
    fn test_repo_languages_no_known_extensions() {
        let files = vec![PathBuf::from("README.md"), PathBuf::from("Makefile")];
        assert!(detect_repo_languages(&files).is_empty());
    }
}
