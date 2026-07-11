//! Language detection from diff content.
//!
//! Scans `diff --git` headers to identify file extensions and map them to
//! programming languages. Provides a primary language determination for use
//! as a template variable in prompt rendering.
//!
//! # Heuristic
//!
//! 1. Parse `diff --git a/path.ext b/path.ext` headers in the diff.
//! 2. Extract file extensions and map them to language names via a lookup table.
//! 3. Count files per language to determine the primary language.
//! 4. Optionally detect framework hints from well-known file paths.
//!
//! # Example
//!
//! ```ignore
//! use crb_tools::language_detector::detect_primary_language;
//!
//! let diff = "diff --git a/src/main.rs b/src/main.rs\n...";
//! assert_eq!(detect_primary_language(diff), "Rust");
//! ```

use std::collections::HashMap;

/// Information about languages detected in a diff.
#[derive(Debug, Clone, Default)]
pub struct LanguageInfo {
    /// The primary programming language (e.g., "Rust", "TypeScript", "Python").
    pub primary_language: String,

    /// All file extensions found (e.g., `[".rs", ".toml"]`).
    pub file_extensions: Vec<String>,

    /// Framework hints (e.g., "Rails", "React", "Django").
    pub framework_hints: Vec<String>,
}

/// Map of file extension (without dot) to language name.
fn extension_to_language() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    // Rust
    map.insert("rs", "Rust");
    map.insert("rlib", "Rust");
    // Go
    map.insert("go", "Go");
    map.insert("mod", "Go"); // go.mod
    // Python
    map.insert("py", "Python");
    map.insert("pyi", "Python");
    map.insert("pyx", "Python");
    // TypeScript / JavaScript
    map.insert("ts", "TypeScript");
    map.insert("tsx", "TypeScript");
    map.insert("js", "JavaScript");
    map.insert("jsx", "JavaScript");
    map.insert("mjs", "JavaScript");
    map.insert("cjs", "JavaScript");
    // Ruby
    map.insert("rb", "Ruby");
    map.insert("erb", "Ruby");
    map.insert("rake", "Ruby");
    // Java
    map.insert("java", "Java");
    map.insert("kt", "Kotlin");
    map.insert("kts", "Kotlin");
    // C / C++
    map.insert("c", "C");
    map.insert("h", "C");
    map.insert("cpp", "C++");
    map.insert("hpp", "C++");
    map.insert("cxx", "C++");
    map.insert("cc", "C++");
    map.insert("hh", "C++");
    // C#
    map.insert("cs", "C#");
    // Swift
    map.insert("swift", "Swift");
    // PHP
    map.insert("php", "PHP");
    // Scala
    map.insert("scala", "Scala");
    // Shell / Scripting
    map.insert("sh", "Shell");
    map.insert("bash", "Shell");
    map.insert("zsh", "Shell");
    map.insert("ps1", "PowerShell");
    // Web / Markup
    map.insert("html", "HTML");
    map.insert("htm", "HTML");
    map.insert("css", "CSS");
    map.insert("scss", "SCSS");
    map.insert("sass", "SCSS");
    map.insert("less", "Less");
    // Config / Data
    map.insert("json", "JSON");
    map.insert("toml", "TOML");
    map.insert("yaml", "YAML");
    map.insert("yml", "YAML");
    map.insert("xml", "XML");
    map.insert("md", "Markdown");
    map.insert("sql", "SQL");
    // Dart
    map.insert("dart", "Dart");
    // Lua
    map.insert("lua", "Lua");
    // Elixir
    map.insert("ex", "Elixir");
    map.insert("exs", "Elixir");
    // Haskell
    map.insert("hs", "Haskell");
    // R
    map.insert("r", "R");
    // Protocol Buffers
    map.insert("proto", "Protocol Buffers");
    // GraphQL
    map.insert("graphql", "GraphQL");
    map.insert("gql", "GraphQL");
    // Docker
    map.insert("dockerfile", "Docker");
    // Makefile
    map.insert("mk", "Makefile");
    map.insert("makefile", "Makefile");
    // Zig
    map.insert("zig", "Zig");
    // Nim
    map.insert("nim", "Nim");
    // Solidity
    map.insert("sol", "Solidity");
    map
}

/// Map of file/directory patterns to framework hints.
fn framework_patterns() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Gemfile", "Ruby on Rails"),
        ("Rakefile", "Ruby on Rails"),
        ("config/routes.rb", "Ruby on Rails"),
        ("app/models/", "Ruby on Rails"),
        ("app/controllers/", "Ruby on Rails"),
        ("app/views/", "Ruby on Rails"),
        ("go.mod", "Go Modules"),
        ("go.sum", "Go Modules"),
        ("package.json", "Node.js"),
        ("tsconfig.json", "TypeScript"),
        ("vite.config.ts", "Vite"),
        ("vite.config.js", "Vite"),
        ("next.config.js", "Next.js"),
        ("next.config.ts", "Next.js"),
        ("Cargo.toml", "Cargo"),
        ("requirements.txt", "pip"),
        ("Pipfile", "pipenv"),
        ("pyproject.toml", "Python"),
        ("manage.py", "Django"),
        ("Django", "Django"),
        ("pom.xml", "Maven"),
        ("build.gradle", "Gradle"),
        ("CMakeLists.txt", "CMake"),
        ("Makefile", "Make"),
        ("Dockerfile", "Docker"),
        ("docker-compose.yml", "Docker Compose"),
        (".github/workflows/", "GitHub Actions"),
        ("Jenkinsfile", "Jenkins"),
    ]
}

/// Extract the file path from a `diff --git a/path b/path` header line.
///
/// Handles standard unified diff headers:
/// - `diff --git a/path b/path`
fn extract_path_from_diff_header(line: &str) -> Option<&str> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("diff --git a/") {
        // Split on " b/" to get the 'a' path
        if let Some(a_path) = rest.split(" b/").next() {
            return Some(a_path);
        }
    }
    None
}

/// Extract file extension from a path.
fn extract_extension(path: &str) -> Option<&str> {
    let path = path.trim();
    // Handle special filenames like "Dockerfile", "Makefile"
    let basename = path.rsplit('/').next().unwrap_or(path);
    let lower = basename.to_lowercase();

    // Special filename -> extension mapping
    match lower.as_str() {
        "dockerfile" => return Some("dockerfile"),
        "makefile" => return Some("makefile"),
        "gemfile" => return Some("gemfile"),
        _ => {}
    }

    // Standard extension extraction
    let (_, ext) = path.rsplit_once('.')?;
    if ext.chars().all(|c| c.is_ascii_alphanumeric()) && ext.len() <= 12 {
        Some(ext)
    } else {
        None
    }
}

/// Detect language information from a git diff string.
///
/// Parses `diff --git` headers to identify file extensions, maps them to
/// languages, and determines the primary language.
pub fn detect_language(diff: &str) -> LanguageInfo {
    let ext_map = extension_to_language();
    let mut lang_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut extensions: Vec<String> = Vec::new();
    let mut seen_extensions: HashMap<String, bool> = HashMap::new();

    for line in diff.lines() {
        if let Some(path) = extract_path_from_diff_header(line) {
            if let Some(ext) = extract_extension(path) {
                let ext_str = ext.to_lowercase();
                if !seen_extensions.contains_key(&ext_str) {
                    seen_extensions.insert(ext_str.clone(), true);
                    extensions.push(ext_str.clone());
                }
                if let Some(&lang) = ext_map.get(ext_str.as_str()) {
                    *lang_counts.entry(lang).or_insert(0) += 1;
                }
            }
        }
    }

    // Determine primary language (most files)
    let primary_language = lang_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(name, _)| name.to_string())
        .unwrap_or_default();

    // Detect framework hints
    let framework_hints: Vec<String> = framework_patterns()
        .iter()
        .filter(|(pattern, _)| diff.contains(pattern))
        .map(|(_, hint)| hint.to_string())
        .collect();

    LanguageInfo {
        primary_language,
        file_extensions: extensions,
        framework_hints,
    }
}

/// Simplified detection that returns just the primary language string.
///
/// Returns an empty string if no language can be determined.
pub fn detect_primary_language(diff: &str) -> String {
    detect_language(diff).primary_language
}

/// Extract repository name from a PR URL.
///
/// Handles URLs like:
/// - `https://github.com/owner/repo/pull/123`
pub fn extract_repo_name(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 2 {
            return parts[1].to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_path_from_diff_header() {
        let line = "diff --git a/src/main.rs b/src/main.rs";
        assert_eq!(extract_path_from_diff_header(line), Some("src/main.rs"));
    }

    #[test]
    fn test_extract_path_with_spaces() {
        let line = "diff --git a/my file.go b/my file.go";
        assert_eq!(extract_path_from_diff_header(line), Some("my file.go"));
    }

    #[test]
    fn test_not_a_diff_header() {
        let line = "--- a/src/main.rs";
        assert_eq!(extract_path_from_diff_header(line), None);
    }

    #[test]
    fn test_extract_extension_rs() {
        assert_eq!(extract_extension("src/main.rs"), Some("rs"));
    }

    #[test]
    fn test_extract_extension_no_ext() {
        assert_eq!(extract_extension("Makefile"), Some("makefile"));
    }

    #[test]
    fn test_extract_extension_dockerfile() {
        assert_eq!(extract_extension("Dockerfile"), Some("dockerfile"));
    }

    #[test]
    fn test_detect_primary_language_rust() {
        let diff = r#"
diff --git a/src/main.rs b/src/main.rs
index abc..def 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,7 @@
 fn main() {
     println!("Hello");
+    println!("World");
 }
diff --git a/src/lib.rs b/src/lib.rs
index 123..456 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 pub fn greet() {
     println!("Hi");
+    println!("Hey");
 }
diff --git a/Cargo.toml b/Cargo.toml
index 123..456 100644
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,3 +1,4 @@
 [package]
 name = "test"
+version = "0.2.0"
"#;
        assert_eq!(detect_primary_language(diff), "Rust");
    }

    #[test]
    fn test_detect_primary_language_typescript() {
        let diff = r#"
diff --git a/src/app.ts b/src/app.ts
index abc..def 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,3 +1,5 @@
 const x = 1;
+const y = 2;
+console.log(x + y);
diff --git a/src/utils.ts b/src/utils.ts
index 123..456 100644
--- a/src/utils.ts
+++ b/src/utils.ts
@@ -1,2 +1,3 @@
 export function add(a: number, b: number) {
+    return a + b;
 }
"#;
        assert_eq!(detect_primary_language(diff), "TypeScript");
    }

    #[test]
    fn test_detect_primary_language_empty_diff() {
        assert_eq!(detect_primary_language(""), "");
    }

    #[test]
    fn test_detect_primary_language_no_diff_headers() {
        let diff = "Some random text\nwith no diff headers\n";
        assert_eq!(detect_primary_language(diff), "");
    }

    #[test]
    fn test_language_info_struct() {
        let diff = r#"
diff --git a/main.rs b/main.rs
index abc..def 100644
--- a/main.rs
+++ b/main.rs
@@ -1 +1,2 @@
 fn main() {}
+// new code
diff --git a/lib.py b/lib.py
--- a/lib.py
+++ b/lib.py
@@ -1 +1,2 @@
 def hello():
+    pass
"#;
        let info = detect_language(diff);
        assert_eq!(info.primary_language, "Rust"); // .rs counted first
        assert!(info.file_extensions.contains(&"rs".to_string()));
        assert!(info.file_extensions.contains(&"py".to_string()));
    }

    #[test]
    fn test_extract_repo_name_standard() {
        let url = "https://github.com/owner/repo/pull/123";
        assert_eq!(extract_repo_name(url), "repo");
    }

    #[test]
    fn test_extract_repo_name_no_match() {
        let url = "not a github url";
        assert_eq!(extract_repo_name(url), "");
    }

    #[test]
    fn test_detect_language_framework_hints() {
        let diff = r#"
diff --git a/Gemfile b/Gemfile
index abc..def 100644
--- a/Gemfile
+++ b/Gemfile
@@ -1 +1,2 @@
 source "https://rubygems.org"
+gem "rails"
diff --git a/app/models/user.rb b/app/models/user.rb
--- a/app/models/user.rb
+++ b/app/models/user.rb
@@ -1 +1,2 @@
 class User < ApplicationRecord
+  validates :name, presence: true
 end
"#;
        let info = detect_language(diff);
        assert_eq!(info.primary_language, "Ruby");
        assert!(
            info.framework_hints.iter().any(|h| h.contains("Rails")),
            "Expected framework hint containing 'Rails', got: {:?}",
            info.framework_hints
        );
    }

    #[test]
    fn test_mixed_languages_counts() {
        let diff = r#"
diff --git a/main.rs b/main.rs
diff --git a/lib.rs b/lib.rs
diff --git a/cli.rs b/cli.rs
diff --git a/server.ts b/server.ts
diff --git a/client.ts b/client.ts
"#;
        assert_eq!(detect_primary_language(diff), "Rust"); // 3 Rust files > 2 TypeScript files
    }
}
