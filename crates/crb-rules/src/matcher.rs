//! Glob matching and language detection for rule files.
//!
//! Provides:
//! - [`rule_matches_path`] - check if a rule's glob patterns match a file path.
//! - [`detect_language`] - map a file path to a [`Language`] variant.
//! - [`Language`] - a type-safe representation of supported programming languages.
//! - [`detect_repo_languages`] - collect unique languages from many paths.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::Rule;

/// A programming language, identified by its GitHub Linguist canonical name.
///
/// Internally stores the official linguist name (e.g. `"Python"`, `"C#"`,
/// `"C++"`, `"TypeScript"`).  Uses linguist-rs naming conventions for
/// compatibility with GitHub's language data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Language(&'static str);

impl Language {
    /// The canonical GitHub Linguist name for this language.
    pub fn name(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

// All language constants use the canonical GitHub Linguist names.
#[allow(non_upper_case_globals)]
impl Language {
    /// Python language.
    pub const Python: Language = Language("Python");
    /// Rust language.
    pub const Rust: Language = Language("Rust");
    /// TypeScript language.
    pub const TypeScript: Language = Language("TypeScript");
    /// JavaScript language.
    pub const JavaScript: Language = Language("JavaScript");
    /// Go language.
    pub const Go: Language = Language("Go");
    /// Ruby language.
    pub const Ruby: Language = Language("Ruby");
    /// Java language.
    pub const Java: Language = Language("Java");
    /// Kotlin language.
    pub const Kotlin: Language = Language("Kotlin");
    /// Swift language.
    pub const Swift: Language = Language("Swift");
    /// C# language.
    pub const CSharp: Language = Language("C#");
    /// C++ language.
    pub const Cpp: Language = Language("C++");
    /// C language.
    pub const C: Language = Language("C");
    /// Scala language.
    pub const Scala: Language = Language("Scala");
    /// PHP language.
    pub const Php: Language = Language("PHP");
}

/// All supported languages, for iteration.
const ALL_LANGUAGES: &[Language] = &[
    Language::Python,
    Language::Rust,
    Language::TypeScript,
    Language::JavaScript,
    Language::Go,
    Language::Ruby,
    Language::Java,
    Language::Kotlin,
    Language::Swift,
    Language::CSharp,
    Language::Cpp,
    Language::C,
    Language::Scala,
    Language::Php,
];

/// Alternative names (aliases) that map to the canonical [`Language`].
///
/// These are accepted by [`language_from_str`] (case-insensitively) alongside
/// the canonical name itself.
const ALIASES: &[(&str, Language)] = &[
    ("csharp", Language::CSharp),
    ("c#", Language::CSharp),
    ("cpp", Language::Cpp),
    ("cxx", Language::Cpp),
    ("c++", Language::Cpp),
    ("cplusplus", Language::Cpp),
    ("php", Language::Php),
    ("typescript", Language::TypeScript),
    ("javascript", Language::JavaScript),
];

/// Canonical mapping from file extension to [`Language`].
///
/// The first extension listed for each language is the "canonical" one used
/// by [`language_to_extension`].
const EXTENSION_MAP: &[(&str, Language)] = &[
    ("py", Language::Python),
    ("rs", Language::Rust),
    ("ts", Language::TypeScript),
    ("tsx", Language::TypeScript),
    ("js", Language::JavaScript),
    ("jsx", Language::JavaScript),
    ("go", Language::Go),
    ("rb", Language::Ruby),
    ("java", Language::Java),
    ("kt", Language::Kotlin),
    ("kts", Language::Kotlin),
    ("swift", Language::Swift),
    ("cs", Language::CSharp),
    ("cpp", Language::Cpp),
    ("cc", Language::Cpp),
    ("cxx", Language::Cpp),
    ("hpp", Language::Cpp),
    ("c", Language::C),
    ("h", Language::C),
    ("scala", Language::Scala),
    ("php", Language::Php),
];

/// Map a file path to a [`Language`] based on its extension.
///
/// Returns `Some(language)` for known extensions, `None` for unknown ones.
pub fn detect_language(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?;
    EXTENSION_MAP
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, lang)| *lang)
}

/// Map a [`Language`] to its canonical file extension.
///
/// Returns the first (most common) extension associated with the language.
pub fn language_to_extension(language: Language) -> &'static str {
    EXTENSION_MAP
        .iter()
        .find(|(_, lang)| *lang == language)
        .map(|(ext, _)| *ext)
        .unwrap_or("txt")
}

/// Collect all unique [`Language`] variants from a slice of file paths.
///
/// Internally calls [`detect_language`] on each path and collects non-`None`
/// results into a [`HashSet`].
pub fn detect_repo_languages(files: &[PathBuf]) -> HashSet<Language> {
    files.iter().filter_map(|f| detect_language(f)).collect()
}

/// Parse a language name string into a [`Language`] variant.
///
/// Accepts the canonical GitHub Linguist name as well as common aliases,
/// case-insensitively.  For example `"Python"`, `"python"`, `"C#"`, `"csharp"`,
/// `"C++"`, `"cpp"`, `"TypeScript"`, `"typescript"`.
pub fn language_from_str(s: &str) -> Option<Language> {
    let lower = s.to_lowercase();

    // Check canonical names first (case-insensitive)
    for lang in ALL_LANGUAGES {
        if lang.0.to_lowercase() == lower {
            return Some(*lang);
        }
    }

    for (alias, lang) in ALIASES {
        if alias.to_lowercase() == lower {
            return Some(*lang);
        }
    }

    None
}

/// Check whether any of `rule`'s glob patterns match `path`.
///
/// Returns `false` immediately if `rule.globs` is empty (empty globs means
/// "no file-path match" — always-apply rules are handled separately by
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(globs: Vec<&str>) -> Rule {
        Rule {
            description: None,
            globs: globs.into_iter().map(String::from).collect(),
            always_apply: false,
            body: String::new(),
            source_file: PathBuf::from("test.md"),
        }
    }

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

    #[test]
    fn test_language_name_returns_canonical_name() {
        assert_eq!(Language::Python.name(), "Python");
        assert_eq!(Language::Rust.name(), "Rust");
        assert_eq!(Language::CSharp.name(), "C#");
        assert_eq!(Language::Cpp.name(), "C++");
    }

    #[test]
    fn test_language_from_str_case_insensitive() {
        // Canonical names (case-insensitive)
        assert_eq!(language_from_str("Python"), Some(Language::Python));
        assert_eq!(language_from_str("python"), Some(Language::Python));
        assert_eq!(language_from_str("PYTHON"), Some(Language::Python));
        assert_eq!(language_from_str("Rust"), Some(Language::Rust));
        assert_eq!(language_from_str("TypeScript"), Some(Language::TypeScript));
        assert_eq!(language_from_str("JavaScript"), Some(Language::JavaScript));

        // Aliases
        assert_eq!(language_from_str("csharp"), Some(Language::CSharp));
        assert_eq!(language_from_str("C#"), Some(Language::CSharp));
        assert_eq!(language_from_str("CSharp"), Some(Language::CSharp));
        assert_eq!(language_from_str("cpp"), Some(Language::Cpp));
        assert_eq!(language_from_str("C++"), Some(Language::Cpp));
        assert_eq!(language_from_str("cplusplus"), Some(Language::Cpp));
        assert_eq!(language_from_str("php"), Some(Language::Php));
    }

    #[test]
    fn test_language_display_uses_canonical_name() {
        assert_eq!(Language::Python.to_string(), "Python");
        assert_eq!(Language::Rust.to_string(), "Rust");
        assert_eq!(Language::CSharp.to_string(), "C#");
        assert_eq!(Language::Cpp.to_string(), "C++");
        assert_eq!(Language::TypeScript.to_string(), "TypeScript");
        assert_eq!(Language::JavaScript.to_string(), "JavaScript");
        assert_eq!(Language::Php.to_string(), "PHP");
    }

    #[test]
    fn test_language_from_str_invalid() {
        assert_eq!(language_from_str("foobar"), None);
        assert_eq!(language_from_str(""), None);
    }

    #[test]
    fn test_language_from_str_function() {
        assert_eq!(language_from_str("Python"), Some(Language::Python));
        assert_eq!(language_from_str("typescript"), Some(Language::TypeScript));
        assert_eq!(language_from_str("unknown"), None);
    }

    #[test]
    fn test_language_equality() {
        assert_eq!(Language::Python, Language::Python);
        assert_ne!(Language::Python, Language::Rust);
    }

    #[test]
    fn test_language_hash_set() {
        let mut set = HashSet::new();
        set.insert(Language::Python);
        set.insert(Language::Rust);
        set.insert(Language::Python); // duplicate
        assert_eq!(set.len(), 2);
        assert!(set.contains(&Language::Python));
        assert!(set.contains(&Language::Rust));
    }

    #[test]
    fn test_detect_python() {
        assert_eq!(
            detect_language(&PathBuf::from("main.py")),
            Some(Language::Python)
        );
    }

    #[test]
    fn test_detect_rust() {
        assert_eq!(
            detect_language(&PathBuf::from("lib.rs")),
            Some(Language::Rust)
        );
    }

    #[test]
    fn test_detect_typescript() {
        assert_eq!(
            detect_language(&PathBuf::from("app.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            detect_language(&PathBuf::from("app.tsx")),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn test_detect_javascript() {
        assert_eq!(
            detect_language(&PathBuf::from("app.js")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            detect_language(&PathBuf::from("app.jsx")),
            Some(Language::JavaScript)
        );
    }

    #[test]
    fn test_detect_go() {
        assert_eq!(
            detect_language(&PathBuf::from("main.go")),
            Some(Language::Go)
        );
    }

    #[test]
    fn test_detect_cpp() {
        assert_eq!(
            detect_language(&PathBuf::from("main.cpp")),
            Some(Language::Cpp)
        );
        assert_eq!(
            detect_language(&PathBuf::from("main.cc")),
            Some(Language::Cpp)
        );
        assert_eq!(
            detect_language(&PathBuf::from("main.hpp")),
            Some(Language::Cpp)
        );
    }

    #[test]
    fn test_detect_c() {
        assert_eq!(detect_language(&PathBuf::from("main.c")), Some(Language::C));
        assert_eq!(detect_language(&PathBuf::from("main.h")), Some(Language::C));
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

    #[test]
    fn test_language_to_extension() {
        assert_eq!(language_to_extension(Language::Python), "py");
        assert_eq!(language_to_extension(Language::Rust), "rs");
        assert_eq!(language_to_extension(Language::Go), "go");
        assert_eq!(language_to_extension(Language::CSharp), "cs");
        assert_eq!(language_to_extension(Language::Cpp), "cpp");
        assert_eq!(language_to_extension(Language::C), "c");
    }

    #[test]
    fn test_repo_languages_from_multiple_files() {
        let files = vec![
            PathBuf::from("src/main.py"),
            PathBuf::from("src/lib.rs"),
            PathBuf::from("src/app.ts"),
            PathBuf::from("README.md"),
        ];
        let langs = detect_repo_languages(&files);
        let mut expected: HashSet<Language> = HashSet::new();
        expected.insert(Language::Python);
        expected.insert(Language::Rust);
        expected.insert(Language::TypeScript);
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
