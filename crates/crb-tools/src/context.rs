//! Language detection from diff content.
//!
//! Scans `diff --git` headers to identify file extensions and map them to programming languages.
//! Provides a primary language determination for use as a template variable in prompt rendering.
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

use std::{collections::HashMap, path::Path};

use crb_shared::diff::Diff;
use crb_types::wrappers::WrappedData;
use linguist::detect_language_by_extension;

/// Information about languages detected in a diff.
#[derive(Debug, Clone, Default)]
pub struct RepoContext {
    /// The detected languages and their counts.
    pub detections: HashMap<&'static str, u16>,

    /// Framework hints.
    pub framework_hints: Vec<String>,
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

impl RepoContext {
    /// Detect from a [`Diff`]
    pub fn from_diff(diff: &Diff) -> Self {
        let mut detections: HashMap<&str, u16> = HashMap::new();
        for section in &diff.sections {
            let Ok(detected_languages) = detect_language_by_extension(&section.path) else {
                continue;
            };

            for detection in detected_languages {
                *detections.entry(detection.name).or_insert(0) += 1;
            }
        }

        let framework_hints: Vec<String> = framework_patterns()
            .iter()
            .filter(|(pattern, _)| diff.get().contains(pattern))
            .map(|(_, hint)| hint.to_string())
            .collect();

        RepoContext {
            detections,
            framework_hints,
        }
    }

    /// Detect from a [`Path`]
    pub fn from_dir(_dir: &Path) -> Self {
        todo!()
    }

    pub fn primary_language(&self) -> Option<&str> {
        self.detections
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(lang, _)| *lang)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            RepoContext::from_diff(&Diff::new(diff.to_string())).primary_language(),
            Some("Rust")
        );
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
        assert_eq!(
            RepoContext::from_diff(&Diff::new(diff.to_string())).primary_language(),
            Some("TypeScript")
        );
    }

    #[test]
    fn test_detect_primary_language_empty_diff() {
        assert_eq!(
            RepoContext::from_diff(&Diff::new("".to_string())).primary_language(),
            None
        );
    }

    #[test]
    fn test_detect_primary_language_no_diff_headers() {
        let diff = "Some random text\nwith no diff headers\n";
        assert_eq!(
            RepoContext::from_diff(&Diff::new(diff.to_string())).primary_language(),
            None
        );
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
        let info = RepoContext::from_diff(&Diff::new(diff.to_string()));
        assert_eq!(info.primary_language(), Some("Rust"));
        assert!(info.detections.contains_key("Rust"));
        assert!(info.detections.contains_key("Python"));
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
        let info = RepoContext::from_diff(&Diff::new(diff.to_string()));
        assert_eq!(info.primary_language(), Some("Ruby"));
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
        assert_eq!(
            RepoContext::from_diff(&Diff::new(diff.to_string())).primary_language(),
            Some("Rust")
        ); // 3 Rust files > 2 TypeScript files
    }
}
