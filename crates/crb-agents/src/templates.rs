//! Handlebars-based template engine for agent prompts.
//!
//! Provides [`TemplateEngine`] as a rich replacement for the simple
//! `{variable}` substitution in [`PromptLibrary`].  Supports conditionals,
//! loops, partials, and custom helpers via the Handlebars crate.

use handlebars::Handlebars;
use serde_json::Value;
use std::collections::HashMap;

/// Create a shared Handlebars registry with common settings and helpers.
///
/// All call sites that need a Handlebars instance should use this factory
/// to ensure consistent configuration (strict mode off, HTML escaping off)
/// and a shared set of helpers (`lowercase`, `uppercase`, `join`).
///
/// Template registration is left to the caller — each use case registers
/// its own templates (e.g. filesystem-based for [`TemplateEngine`],
/// embedded for [`PromptLibrary`](crate::prompts::PromptLibrary)).
pub(crate) fn new_handlebars_registry() -> Handlebars<'static> {
    let mut registry = Handlebars::new();
    registry.set_strict_mode(false);
    // Disable HTML escaping — markdown content often contains backticks,
    // equals signs, and apostrophes that must be passed through raw.
    registry.register_escape_fn(handlebars::no_escape);

    registry.register_helper(
        "lowercase",
        Box::new(
            |h: &handlebars::Helper,
             _: &handlebars::Handlebars,
             _: &handlebars::Context,
             _: &mut handlebars::RenderContext,
             out: &mut dyn handlebars::Output|
             -> handlebars::HelperResult {
                let param = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("");
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
                let param = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("");
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
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                    .unwrap_or_default();
                let separator = h.param(1).and_then(|v| v.value().as_str()).unwrap_or(", ");
                out.write(&items.join(separator))?;
                Ok(())
            },
        ),
    );

    registry
}

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
    /// rather than failing.  A `lowercase`, `uppercase`, and `join` helper are
    /// registered by default.
    pub fn new() -> Self {
        let registry = new_handlebars_registry();
        Self { registry }
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
        engine.register_template("test", "Hello {{name}}!").unwrap();
        assert!(engine.has_template("test"));

        let ctx = serde_json::json!({"name": "World"});
        let result = engine.render("test", &ctx).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_template_engine_render_with_if() {
        let mut engine = TemplateEngine::new();
        engine
            .register_template("test", "{{#if show}}Visible{{else}}Hidden{{/if}}")
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
        extra.insert("file_list".to_string(), serde_json::json!(["a.rs", "b.rs"]));
        let ctx = build_template_context("python", "my-repo", "CL", 15, Some(extra));
        assert_eq!(ctx["diff"], "some diff");
        assert_eq!(ctx["file_list"][0], "a.rs");
    }

    #[test]
    fn test_template_names() {
        let mut engine = TemplateEngine::new();
        engine.register_template("alpha", "content a").unwrap();
        engine.register_template("beta", "content b").unwrap();
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
