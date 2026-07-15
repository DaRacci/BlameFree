use anyhow::Result;
use handlebars::Handlebars;
use include_dir::{Dir, include_dir};
use std::collections::HashMap;
use tracing::warn;

use crate::{agent::AgentEntry, templates};

static PROMPT_LIBRARY: Option<PromptLibrary> = None;
static PROMPTS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../prompts");

const AGENT_TEMPLATE_PATH: &str = "agent.hbs";
const AGENTS_DIR: &str = "agents";
const SECTIONS_DIR: &str = "sections";

/// A library of role prompts loaded from embedded templates and markdown files.
///
/// The [`PromptLibrary`] is initialised at compile time via `include_dir!` and
/// provides:
/// - Role-specific agent prompt rendering through a Handlebars template.
/// - Section content injection (output_format, max_findings, etc.).
/// - Agent metadata lookups (config, raw body).
#[derive(Clone)]
pub struct PromptLibrary {
    /// Map of uppercase abbreviation -> agent entry.
    agents: HashMap<String, AgentEntry>,

    /// Map of section name -> raw section content.
    sections: HashMap<String, String>,

    /// Compiled Handlebars registry with the agent template registered.
    handlebars: Handlebars<'static>,
}

impl PromptLibrary {
    /// Gets a reference to the static `PromptLibrary`.
    pub fn get_instance() -> &'static Self {
        PROMPT_LIBRARY.as_ref().unwrap()
    }

    /// Initialise from embedded data.
    ///
    /// This function will initialise the static `PROMPT_LIBRARY` if it hasn't been initialised yet, and return a reference to it.
    /// Returns error if agent.hbs is missing or no agents are found.
    pub fn new() -> Result<&'static Self, String> {
        if PROMPT_LIBRARY.is_some() {
            return Ok(Self::get_instance());
        }

        let agent_template = PROMPTS_DIR
            .get_file(AGENT_TEMPLATE_PATH)
            .ok_or("agent.hbs not found in embedded prompts")?
            .contents_utf8()
            .ok_or("agent.hbs is not valid UTF-8")?
            .to_string();

        let mut sections: HashMap<String, String> = HashMap::new();
        if let Some(sections_dir) = PROMPTS_DIR.get_dir(SECTIONS_DIR) {
            for entry in sections_dir.files() {
                let name = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                let content = entry.contents_utf8().unwrap_or("").to_string();
                sections.insert(name.to_string(), content);
            }
        }

        let mut agents: HashMap<String, AgentEntry> = HashMap::new();
        if let Some(agents_dir) = PROMPTS_DIR.get_dir(AGENTS_DIR) {
            for file in agents_dir.files() {
                if file.path().extension().is_none_or(|e| e != "md") {
                    continue;
                }
                let content = file.contents_utf8().unwrap_or("");
                let entry = AgentEntry::new(content).map_err(|e| {
                    format!(
                        "Failed to parse agent file '{}': {e}",
                        file.path().display()
                    )
                })?;
                agents.insert(entry.role_abbreviation.to_uppercase(), entry);
            }
        }

        if agents.is_empty() {
            return Err("No agents found in embedded prompts".into());
        }

        let mut hb = templates::new_handlebars_registry();
        hb.register_template_string("agent", &agent_template)
            .map_err(|e| format!("Failed to register agent template: {e}"))?;

        PROMPT_LIBRARY.as_ref().replace(&PromptLibrary {
            agents,
            sections,
            handlebars: hb,
        });

        Ok(Self::get_instance())
    }

    /// Get the raw markdown body for a role with the YAML stripped.
    pub fn get(&self, role: &str) -> Option<&str> {
        self.agents
            .get(&role.to_uppercase())
            .map(|e| e.role_prompt.as_str())
    }

    /// Get the agent entry for a role.
    pub fn config(&self, role: &str) -> Option<&AgentEntry> {
        self.agents.get(&role.to_uppercase())
    }

    /// Render an agents prompt through the agent template.
    ///
    /// `vars` provides runtime variables like `diff`, `file_list`, `language`
    ///
    /// Agent context comes from the embedded YAML frontmatter.
    ///
    /// Section content (output_format, max_findings, submit_finding) is
    /// rendered with the current context and injected as variables so that
    /// `agent.hbs` can reference them with `{{output_format}}` etc.
    pub fn render(&self, agent: &AgentEntry, vars: HashMap<String, serde_json::Value>) -> String {
        let mut ctx = serde_json::Map::new();
        vars.into_iter().for_each(|(k, v)| {
            ctx.insert(k, v);
        });

        let fields = AgentEntry::SERDE_FIELDS;
        for field in fields {
            ctx.insert(
                field.to_string(),
                match *field {
                    "role_name" => agent.role_name.clone(),
                    "role_abbreviation" => agent.role_abbreviation.clone(),
                    "role_domain" => agent.role_domain.clone(),
                    "role_anti_hallucination_rules" => agent
                        .role_anti_hallucination_rules
                        .clone()
                        .unwrap_or_default(),
                    "role_review_methodology" => {
                        agent.role_review_methodology.clone().unwrap_or_default()
                    }
                    "role_prompt" => agent.role_prompt.clone(),
                    _ => continue,
                }
                .into(),
            );
        }

        // Sections themselves may contain template variables,
        // so render each section through the engine first.
        let base_ctx = serde_json::Value::Object(ctx.clone());
        for (name, content) in &self.sections {
            if !ctx.contains_key(name) {
                let rendered = self
                    .handlebars
                    .render_template(content, &base_ctx)
                    .unwrap_or_else(|_| content.clone());
                ctx.insert(name.clone(), rendered.into());
            }
        }

        let ctx_value = serde_json::Value::Object(ctx);
        self.handlebars
            .render("agent", &ctx_value)
            .unwrap_or_else(|e| {
                warn!(
                    "Failed to render agent template for '{}': {e}",
                    agent.role_name
                );
                format!("Error rendering prompt for {}: {e}", agent.role_name)
            })
    }

    /// List all known agent entries.
    pub fn agents(&self) -> Vec<&AgentEntry> {
        self.agents.values().collect()
    }

    /// List all known agent abbreviations.
    pub fn abbreviations(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }

    /// Get the generalist agent entry, if available.
    pub fn generalist(&self) -> Option<&AgentEntry> {
        self.agents.values().find(|e| e.generalist_agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_library_loads() {
        let lib = PromptLibrary::get_instance();
        assert!(!lib.agents().is_empty());
        assert!(lib.get("SA").is_some() || lib.get("GEN").is_some());
    }
}
