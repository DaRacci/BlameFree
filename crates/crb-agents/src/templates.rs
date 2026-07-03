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

use crate::manifest::{AgentEntry, AgentManifest};

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
        // Disable HTML escaping — markdown content often contains backticks,
        // equals signs, and apostrophes that must be passed through raw.
        registry.register_escape_fn(handlebars::no_escape);

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

/// Load an [`AgentManifest`] and full template engine from the new agent layout.
///
/// 1. Loads `AgentManifest` from `agents_dir` (e.g. `prompts/agents/`).
/// 2. Registers `agent.hbs` as the primary template (loaded from `builtin_dir`).
/// 3. Registers all `.hbs` section files from `sections_dir` as templates
///    that can be rendered individually (also used as context data).
///
/// Returns a tuple of `(TemplateEngine, AgentManifest)`.
///
/// # Errors
///
/// Returns an error if any directory is missing, files cannot be read, or
/// the manifest validation fails.
pub fn try_load_agent_engine(
    agents_dir: &Path,
    builtin_dir: &Path,
    sections_dir: &Path,
) -> anyhow::Result<(TemplateEngine, AgentManifest)> {
    // 1. Load agent manifest
    let manifest = AgentManifest::load_from_dir(agents_dir)?;
    tracing::info!(
        "Loaded {} agent manifests from {}",
        manifest.all_abbreviations().len(),
        agents_dir.display()
    );

    // 2. Create template engine
    let mut engine = TemplateEngine::new();

    // 3. Register agent.hbs as primary template
    let agent_hbs_path = builtin_dir.join("agent.hbs");
    if agent_hbs_path.exists() {
        let content = std::fs::read_to_string(&agent_hbs_path)?;
        engine.register_template("agent", &content)?;
        tracing::info!("Registered primary template from {}", agent_hbs_path.display());
    } else {
        anyhow::bail!(
            "Primary template 'agent.hbs' not found at {}",
            agent_hbs_path.display()
        );
    }

    // 4. Register section .hbs templates as partials/renderable templates
    if sections_dir.exists() {
        engine.load_dir(sections_dir)?;
        tracing::info!("Loaded section templates from {}", sections_dir.display());
    }

    // 5. Register any additional .hbs templates from builtin_dir
    let existing_templates: std::collections::HashSet<String> =
        engine.template_names().into_iter().collect();
    for entry in std::fs::read_dir(builtin_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "hbs") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            if !existing_templates.contains(&stem) && stem != "agent" {
                let content = std::fs::read_to_string(&path)?;
                engine.register_template(&stem, &content)?;
            }
        }
    }

    Ok((engine, manifest))
}

/// Build the rendering context for an agent from its manifest entry, sections,
/// and optional extra variables.
///
/// This function:
/// 1. Starts with the agent's frontmatter fields (`role_name`, `role_abbreviation`,
///    `role_domain`, `role_anti_hallucination_rules`, `role_review_methodology`,
///    `role_prompt`).
/// 2. Renders each section template (`.hbs` and `.md` files from `sections_dir`)
///    with the current context, then places the rendered output into the context
///    under the section's filename stem.
/// 3. Returns the complete JSON context for rendering `agent.hbs`.
///
/// # Arguments
///
/// * `engine` - The template engine (used to render section templates).
/// * `entry` - The agent manifest entry.
/// * `sections_dir` - Directory containing section templates/files.
/// * `max_findings` - Maximum findings limit (passed to section templates).
/// * `extra_vars` - Optional extra variables merged into the context.
pub fn build_agent_context(
    engine: &TemplateEngine,
    entry: &AgentEntry,
    sections_dir: &Path,
    max_findings: usize,
    extra_vars: Option<HashMap<String, Value>>,
) -> anyhow::Result<Value> {
    let mut ctx = serde_json::json!({
        "role_name": entry.role_name,
        "role_abbreviation": entry.role_abbreviation,
        "role_domain": entry.role_domain,
        "role_anti_hallucination_rules": entry.role_anti_hallucination_rules,
        "role_review_methodology": entry.role_review_methodology,
        "role_prompt": entry.role_prompt,
    });

    // Start building the section rendering context with basic agent info
    let mut section_ctx = serde_json::json!({
        "role_name": entry.role_name,
        "role_abbreviation": entry.role_abbreviation,
        "role_domain": entry.role_domain,
        "role_anti_hallucination_rules": entry.role_anti_hallucination_rules,
        "role_review_methodology": entry.role_review_methodology,
        "role_prompt": entry.role_prompt,
        "max_findings": max_findings,
    });
    if let Some(ref extra) = extra_vars {
        if let Some(obj) = section_ctx.as_object_mut() {
            for (k, v) in extra {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    // Render and inject section files into context
    if sections_dir.exists() {
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(sections_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension()
                    .map_or(false, |ext| ext == "hbs" || ext == "md")
            })
            .collect();
        entries.sort();

        for path in &entries {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            // Map submit_findings → submit_finding for template compatibility
            let context_key = if stem == "submit_findings" {
                "submit_finding".to_string()
            } else {
                stem.clone()
            };

            let rendered = if engine.has_template(&stem) {
                // .hbs sections: render via handlebars with section context
                engine.render(&stem, &section_ctx)?
            } else {
                // .md sections: just read the raw content
                std::fs::read_to_string(path)?
            };

            if let Some(obj) = ctx.as_object_mut() {
                obj.insert(context_key, Value::String(rendered));
            }
        }
    }

    // Merge extra_vars into final context (overwrites any section keys)
    if let Some(extra) = extra_vars {
        if let Some(obj) = ctx.as_object_mut() {
            for (k, v) in extra {
                obj.insert(k, v);
            }
        }
    }

    Ok(ctx)
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
