//! File-based prompt library with template support.
//!
//! Provides a [`PromptLibrary`] that loads prompts from markdown files and
//! falls back to built-in defaults when no file-based prompt is available.
//! Supports simple `{variable}` template substitution.

use std::collections::HashMap;
use std::path::Path;

/// Manages prompt templates loaded from files or built-in defaults.
#[derive(Clone)]
pub struct PromptLibrary {
    /// Built-in defaults (fallback when no file-based prompt exists).
    defaults: HashMap<String, String>,
    /// Custom prompts loaded from a directory.
    custom: HashMap<String, String>,
}

impl PromptLibrary {
    /// Create a new `PromptLibrary` with only built-in defaults.
    pub fn new() -> Self {
        Self {
            defaults: builtin_defaults(),
            custom: HashMap::new(),
        }
    }

    /// Load custom prompts from a directory.
    ///
    /// Scans for files named `{role}.md` (e.g. `sa.md`, `cl.md`, `ar.md`, `sec.md`)
    /// and reads their contents into the custom prompt map.  The role key is the
    /// filename stem, uppercased (e.g. `sa.md` → `"SA"`).
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be read or a file cannot be
    /// opened.
    pub fn load_from_dir(&mut self, dir: &Path) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_uppercase())
                    .unwrap_or_default();
                let content = std::fs::read_to_string(&path)?;
                let trimmed = content.trim().to_string();
                self.custom.insert(stem, trimmed);
            }
        }
        Ok(())
    }

    /// Get the prompt for a role. Returns a custom prompt if available,
    /// otherwise falls back to the built-in default.
    pub fn get(&self, role: &str) -> &str {
        let role_upper = role.to_uppercase();
        self.custom
            .get(&role_upper)
            .or_else(|| {
                // Also check lowercase key in custom
                self.custom.get(role)
            })
            .map(|s| s.as_str())
            .unwrap_or_else(|| {
                self.defaults
                    .get(&role_upper)
                    .map(|s| s.as_str())
                    .unwrap_or_else(|| {
                        self.defaults
                            .get("DEFAULT")
                            .map(|s| s.as_str())
                            .unwrap_or("")
                    })
            })
    }

    /// Apply template variables to the prompt for a role.
    ///
    /// Replaces all occurrences of `{variable_name}` with the corresponding
    /// value from `vars`.  Variables not found in `vars` are left as-is.
    pub fn render(&self, role: &str, vars: &HashMap<&str, &str>) -> String {
        let prompt = self.get(role);
        let mut result = prompt.to_string();
        for (key, value) in vars {
            let placeholder = format!("{{{}}}", key);
            result = result.replace(&placeholder, value);
        }
        result
    }
}

impl Default for PromptLibrary {
    fn default() -> Self {
        Self::new()
    }
}

/// Create the built-in defaults map with the same content as the original
/// `const &str` preambles in `lib.rs`.
pub fn builtin_defaults() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert(
        "SA".to_string(),
        "You are a static analysis specialist. Analyze the provided code diff for \
potential bugs, code smells, and violations of best practices. Focus on \
correctness, error handling, and code quality issues. Respond with a JSON \
array of findings."
            .to_string(),
    );
    m.insert(
        "CL".to_string(),
        "You are a code logic expert. Examine the diff for logical errors, incorrect \
assumptions, off-by-one errors, race conditions, and concurrency issues. \
Focus on whether the code correctly implements its intended logic. Respond \
with a JSON array of findings."
            .to_string(),
    );
    m.insert(
        "AR".to_string(),
        "You are an architecture reviewer. Evaluate the diff for architectural concerns: \
coupling, cohesion, separation of concerns, design pattern violations, and \
maintainability issues. Focus on the high-level structure and design decisions. \
Respond with a JSON array of findings."
            .to_string(),
    );
    m.insert(
        "SEC".to_string(),
        "You are a security specialist. Review the diff for security vulnerabilities: \
injection flaws, authentication/authorization issues, data exposure, input \
validation problems, and other security weaknesses. Focus on OWASP Top 10 \
categories. Respond with a JSON array of findings."
            .to_string(),
    );
    m.insert(
        "DEFAULT".to_string(),
        "You are a code reviewer. Analyze the provided code diff and identify any \
issues. Respond with a JSON array of findings."
            .to_string(),
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_prompt_library_defaults() {
        let lib = PromptLibrary::new();
        assert_eq!(lib.get("SA"), lib.defaults.get("SA").unwrap());
        assert_eq!(lib.get("CL"), lib.defaults.get("CL").unwrap());
        assert_eq!(lib.get("AR"), lib.defaults.get("AR").unwrap());
        assert_eq!(lib.get("SEC"), lib.defaults.get("SEC").unwrap());
    }

    #[test]
    fn test_prompt_library_fallback_to_default() {
        let lib = PromptLibrary::new();
        // Unrecognized role falls back to DEFAULT
        let default = lib.get("UNKNOWN");
        assert_eq!(default, lib.defaults.get("DEFAULT").unwrap());
    }

    #[test]
    fn test_prompt_library_load_from_dir() -> anyhow::Result<()> {
        // Create a temp dir with custom prompts
        let dir = std::env::temp_dir().join("prompts_test").join("sa.md");
        std::fs::create_dir_all(dir.parent().unwrap())?;
        std::fs::write(&dir, "Custom SA prompt")?;

        let mut lib = PromptLibrary::new();
        lib.load_from_dir(dir.parent().unwrap())?;

        assert_eq!(lib.get("SA"), "Custom SA prompt");

        // Clean up
        std::fs::remove_dir_all(dir.parent().unwrap())?;
        Ok(())
    }

    #[test]
    fn test_prompt_library_custom_overrides_default() -> anyhow::Result<()> {
        let dir = std::env::temp_dir().join("prompts_test_over").join("sa.md");
        std::fs::create_dir_all(dir.parent().unwrap())?;
        std::fs::write(&dir, "Custom override SA")?;

        let mut lib = PromptLibrary::new();
        lib.load_from_dir(dir.parent().unwrap())?;

        // Custom should be returned instead of default
        assert_eq!(lib.get("SA"), "Custom override SA");
        assert_ne!(lib.get("SA"), lib.defaults.get("SA").unwrap());

        std::fs::remove_dir_all(dir.parent().unwrap())?;
        Ok(())
    }

    #[test]
    fn test_prompt_library_render_no_vars() {
        let lib = PromptLibrary::new();
        let vars = HashMap::new();
        let rendered = lib.render("SA", &vars);
        assert_eq!(rendered, lib.defaults.get("SA").unwrap().as_str());
    }

    #[test]
    fn test_prompt_library_render_with_vars() {
        let mut lib = PromptLibrary::new();
        // Insert a custom prompt with a template variable
        lib.custom.insert(
            "SA".to_string(),
            "Analyze {diff} for {role} role".to_string(),
        );

        let mut vars = HashMap::new();
        vars.insert("diff", "the code");
        vars.insert("role", "SA");

        let rendered = lib.render("SA", &vars);
        assert_eq!(rendered, "Analyze the code for SA role");
    }

    #[test]
    fn test_prompt_library_render_missing_var_left_as_is() {
        let mut lib = PromptLibrary::new();
        lib.custom.insert(
            "SA".to_string(),
            "Analyze {diff} for {role}".to_string(),
        );

        let mut vars = HashMap::new();
        vars.insert("diff", "the code");
        // Note: {role} is NOT in vars — should be left as-is

        let rendered = lib.render("SA", &vars);
        assert_eq!(rendered, "Analyze the code for {role}");
    }

    #[test]
    fn test_builtin_defaults_contains_all_roles() {
        let defaults = builtin_defaults();
        assert!(defaults.contains_key("SA"));
        assert!(defaults.contains_key("CL"));
        assert!(defaults.contains_key("AR"));
        assert!(defaults.contains_key("SEC"));
        assert!(defaults.contains_key("DEFAULT"));
        assert_eq!(defaults.len(), 5);
    }

    #[test]
    fn test_prompt_library_default_impl() {
        let lib: PromptLibrary = Default::default();
        assert_eq!(lib.get("SA"), lib.defaults.get("SA").unwrap());
    }
}
