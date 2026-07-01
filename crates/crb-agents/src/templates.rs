//! Handlebars-based template engine for agent prompts.
//!
//! Provides [`TemplateEngine`] as a rich replacement for the simple
//! `{variable}` substitution in [`PromptLibrary`].  Supports conditionals,
//! loops, partials, and custom helpers via the Handlebars crate.
//!
//! # Usage
//!
//! ```ignore
//! use crb_agents::templates::TemplateEngine;
//! use std::path::Path;
//!
//! let mut engine = TemplateEngine::new();
//! engine.load_dir(Path::new("prompts/builtin/handlebars")).unwrap();
//!
//! let ctx = serde_json::json!({
//!     "language": "rust",
//!     "role": "SA",
//!     "exp14_submit_finding": true,
//! });
//! let prompt = engine.render("sa", &ctx).unwrap();
//! ```

use handlebars::Handlebars;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Template engine that loads and renders Handlebars templates.
///
/// Templates are registered by name (filename stem without `.hbs`).
/// Use [`render`](Self::render) to produce the final prompt string.
#[derive(Clone)]
pub struct TemplateEngine {
    registry: Handlebars<'static>,
}

impl TemplateEngine {
    /// Create a new empty template engine.
    ///
    /// Strict mode is disabled so missing variables render as empty strings
    /// rather than failing.  A `lowercase` and `uppercase` helper are
    /// registered by default.
    pub fn new() -> Self {
        let mut registry = Handlebars::new();
        registry.set_strict_mode(false);

        // Register built-in helpers
        registry.register_helper(
            "lowercase",
            Box::new(
                |h: &handlebars::Helper,
                 _: &handlebars::Handlebars,
                 _: &handlebars::Context,
                 _: &mut handlebars::RenderContext,
                 out: &mut dyn handlebars::Output|
                 -> handlebars::HelperResult {
                    let param = h
                        .param(0)
                        .and_then(|v| v.value().as_str())
                        .unwrap_or("");
                    out.write(&param.to_lowercase())?;
                    Ok(())
                },
            ),
        );

        registry.register_helper(
            "uppercase",
            Box::new(
                |h: &handlebars::Helper,
                 _: &handlebars::Handlebars,
                 _: &handlebars::Context,
                 _: &mut handlebars::RenderContext,
                 out: &mut dyn handlebars::Output|
                 -> handlebars::HelperResult {
                    let param = h
                        .param(0)
                        .and_then(|v| v.value().as_str())
                        .unwrap_or("");
                    out.write(&param.to_uppercase())?;
                    Ok(())
                },
            ),
        );

        // Register 'join' helper for arrays
        registry.register_helper(
            "join",
            Box::new(
                |h: &handlebars::Helper,
                 _: &handlebars::Handlebars,
                 _: &handlebars::Context,
                 _: &mut handlebars::RenderContext,
                 out: &mut dyn handlebars::Output|
                 -> handlebars::HelperResult {
                    let items = h
                        .param(0)
                        .and_then(|v| v.value().as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let separator = h.param(1).and_then(|v| v.value().as_str()).unwrap_or(", ");
                    out.write(&items.join(separator))?;
                    Ok(())
                },
            ),
        );

        Self { registry }
    }

    /// Load all `.hbs` templates from a directory.
    ///
    /// Each file is registered as a template named by its filename stem
    /// (without the `.hbs` extension).  For example, `sa.hbs` becomes
    /// available for rendering as `"sa"`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be read or any file cannot
    /// be opened or parsed.
    pub fn load_dir(&mut self, dir: &Path) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "hbs") {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                let content = std::fs::read_to_string(&path)?;
                self.registry.register_template_string(&stem, content)?;
            }
        }
        Ok(())
    }

    /// Register a single template by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the template content cannot be parsed.
    pub fn register_template(&mut self, name: &str, content: &str) -> anyhow::Result<()> {
        self.registry.register_template_string(name, content)?;
        Ok(())
    }

    /// Render a template with the given context variables.
    ///
    /// Returns the rendered string or an error if the template is not found
    /// or rendering fails.
    ///
    /// # Errors
    ///
    /// Returns an error if the template name is not registered or if
    /// rendering fails due to a syntax error in the template.
    pub fn render(&self, name: &str, vars: &Value) -> anyhow::Result<String> {
        Ok(self.registry.render(name, vars)?)
    }

    /// Check if a template is registered.
    pub fn has_template(&self, name: &str) -> bool {
        self.registry.has_template(name)
    }

    /// Return the list of registered template names.
    pub fn template_names(&self) -> Vec<String> {
        self.registry
            .get_templates()
            .keys()
            .map(|k| k.to_string())
            .collect()
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a rich context object for template rendering.
///
/// Merges common variables (language, repo, role, max_findings, feature flags)
/// with any custom variables provided by the caller.
pub fn build_template_context(
    language: &str,
    repo_name: &str,
    role: &str,
    max_findings: usize,
    extra_vars: Option<HashMap<String, Value>>,
) -> Value {
    let mut ctx = serde_json::json!({
        "language": language,
        "repo": repo_name,
        "role": role,
        "max_findings": max_findings,
        "exp14_submit_finding": cfg!(feature = "exp14_submit_finding"),
    });

    if let Some(extra) = extra_vars {
        if let Some(obj) = ctx.as_object_mut() {
            for (k, v) in extra {
                obj.insert(k, v);
            }
        }
    }

    ctx
}

/// Auto-detect which prompt engine to use based on environment / filesystem.
///
/// Returns the path to the handlebars directory if:
/// 1. `CRB_PROMPT_ENGINE` env var is not set to `"legacy"`, AND
/// 2. `prompts/builtin/handlebars/` exists relative to the current dir.
///
/// Falls back to legacy otherwise.
pub fn auto_detect_handlebars_path() -> Option<std::path::PathBuf> {
    let engine = std::env::var("CRB_PROMPT_ENGINE").unwrap_or_default();
    if engine == "legacy" {
        return None;
    }

    let hbs_dir = std::path::PathBuf::from("prompts/builtin/handlebars");
    if hbs_dir.exists() {
        Some(hbs_dir)
    } else {
        None
    }
}

/// Try to load a TemplateEngine from the auto-detected handlebars path.
///
/// Returns `Some(engine)` if the handlebars directory exists and templates
/// can be loaded.  Returns `None` if the directory doesn't exist or loading
/// fails (a warning is emitted via `tracing`).
pub fn try_load_template_engine() -> Option<TemplateEngine> {
    let path = auto_detect_handlebars_path()?;
    let mut engine = TemplateEngine::new();
    match engine.load_dir(&path) {
        Ok(()) => {
            let names = engine.template_names();
            tracing::info!(
                "Loaded {} handlebars templates from {}: {:?}",
                names.len(),
                path.display(),
                names
            );
            Some(engine)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to load handlebars templates from {}: {}. Falling back to legacy.",
                path.display(),
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_template_engine_new() {
        let engine = TemplateEngine::new();
        assert!(!engine.has_template("sa"));
    }

    #[test]
    fn test_template_engine_register_and_render() {
        let mut engine = TemplateEngine::new();
        engine
            .register_template("test", "Hello {{name}}!")
            .unwrap();
        assert!(engine.has_template("test"));

        let ctx = serde_json::json!({"name": "World"});
        let result = engine.render("test", &ctx).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_template_engine_render_with_if() {
        let mut engine = TemplateEngine::new();
        engine
            .register_template(
                "test",
                "{{#if show}}Visible{{else}}Hidden{{/if}}",
            )
            .unwrap();

        let ctx_true = serde_json::json!({"show": true});
        assert_eq!(engine.render("test", &ctx_true).unwrap(), "Visible");

        let ctx_false = serde_json::json!({"show": false});
        assert_eq!(engine.render("test", &ctx_false).unwrap(), "Hidden");
    }

    #[test]
    fn test_template_engine_render_with_each() {
        let mut engine = TemplateEngine::new();
        engine
            .register_template(
                "test",
                "{{#each items}}{{this}}{{#unless @last}}, {{/unless}}{{/each}}",
            )
            .unwrap();

        let ctx = serde_json::json!({"items": ["a", "b", "c"]});
        let result = engine.render("test", &ctx).unwrap();
        assert_eq!(result, "a, b, c");
    }

    #[test]
    fn test_lowercase_helper() {
        let mut engine = TemplateEngine::new();
        engine
            .register_template("test", "{{lowercase text}}")
            .unwrap();
        let ctx = serde_json::json!({"text": "Hello World"});
        assert_eq!(engine.render("test", &ctx).unwrap(), "hello world");
    }

    #[test]
    fn test_uppercase_helper() {
        let mut engine = TemplateEngine::new();
        engine
            .register_template("test", "{{uppercase text}}")
            .unwrap();
        let ctx = serde_json::json!({"text": "Hello World"});
        assert_eq!(engine.render("test", &ctx).unwrap(), "HELLO WORLD");
    }

    #[test]
    fn test_join_helper() {
        let mut engine = TemplateEngine::new();
        engine
            .register_template("test", "{{join items \", \"}}")
            .unwrap();
        let ctx = serde_json::json!({"items": ["a", "b", "c"]});
        assert_eq!(engine.render("test", &ctx).unwrap(), "a, b, c");
    }

    #[test]
    fn test_build_template_context() {
        let ctx = build_template_context("rust", "my-repo", "SA", 20, None);
        assert_eq!(ctx["language"], "rust");
        assert_eq!(ctx["repo"], "my-repo");
        assert_eq!(ctx["role"], "SA");
        assert_eq!(ctx["max_findings"], 20);
    }

    #[test]
    fn test_build_template_context_with_extra() {
        let mut extra = HashMap::new();
        extra.insert("diff".to_string(), serde_json::json!("some diff"));
        extra.insert(
            "file_list".to_string(),
            serde_json::json!(["a.rs", "b.rs"]),
        );
        let ctx = build_template_context("python", "my-repo", "CL", 15, Some(extra));
        assert_eq!(ctx["diff"], "some diff");
        assert_eq!(ctx["file_list"][0], "a.rs");
    }

    #[test]
    fn test_auto_detect_handlebars_path() {
        // Without the env var set and without the directory, should return None
        let original = std::env::var("CRB_PROMPT_ENGINE").ok();
        std::env::remove_var("CRB_PROMPT_ENGINE");
        // In a test context the dir probably doesn't exist
        let result = auto_detect_handlebars_path();
        // Either None or Some — depends on whether we're running from the project root
        // We just verify it doesn't panic and returns a valid PathBuf if Some
        if let Some(path) = result {
            assert!(path.ends_with("handlebars"));
        }
        // Restore
        if let Some(val) = original {
            std::env::set_var("CRB_PROMPT_ENGINE", val);
        }
    }

    #[test]
    fn test_template_engine_load_dir() {
        use std::path::PathBuf;
        // Create a temp dir with a .hbs file
        let dir = std::env::temp_dir().join("hbs_test_load_dir");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test.hbs"), "Hello {{name}}!").unwrap();

        let mut engine = TemplateEngine::new();
        engine.load_dir(&dir).unwrap();
        assert!(engine.has_template("test"));

        let ctx = serde_json::json!({"name": "Engine"});
        assert_eq!(engine.render("test", &ctx).unwrap(), "Hello Engine!");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_template_names() {
        let mut engine = TemplateEngine::new();
        engine
            .register_template("alpha", "content a")
            .unwrap();
        engine
            .register_template("beta", "content b")
            .unwrap();
        let mut names = engine.template_names();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_render_missing_template_returns_error() {
        let engine = TemplateEngine::new();
        let ctx = serde_json::json!({});
        let result = engine.render("nonexistent", &ctx);
        assert!(result.is_err());
    }
}
