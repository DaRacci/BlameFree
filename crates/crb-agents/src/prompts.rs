use handlebars::Handlebars;
use include_dir::{include_dir, Dir};
use std::collections::HashMap;

use crate::manifest::{split_frontmatter, AgentEntry};

static PROMPTS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../prompts");
const AGENT_TEMPLATE_PATH: &str = "agent.hbs";
const AGENTS_DIR: &str = "agents";
const SECTIONS_DIR: &str = "sections";

#[derive(Clone)]
pub struct PromptLibrary {
    agents: HashMap<String, AgentEntry>,
    sections: HashMap<String, String>,
    handlebars: Handlebars<'static>,
}

impl PromptLibrary {
    /// Initialise from embedded data. Returns error if agent.hbs is missing
    /// or no agents are found.
    pub fn new() -> Result<Self, String> {
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
                if let Some((yaml_str, body)) = split_frontmatter(content) {
                    let mut entry: AgentEntry = serde_yaml::from_str(yaml_str).unwrap_or_default();
                    if entry.role_abbreviation.is_empty() {
                        continue;
                    }
                    let clean_body = body.trim().to_string();
                    entry.role_prompt = clean_body;
                    agents.insert(entry.role_abbreviation.to_uppercase(), entry);
                }
            }
        }

        if agents.is_empty() {
            return Err("No agents found in embedded prompts".into());
        }

        let mut hb = crate::templates::new_handlebars_registry();
        hb.register_template_string("agent", &agent_template)
            .map_err(|e| format!("Failed to register agent template: {e}"))?;

        Ok(Self {
            agents,
            sections,
            handlebars: hb,
        })
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

    /// Render a role's prompt through agent.hbs.
    ///
    /// `vars` provides runtime variables like `diff`, `file_list`, `language`
    ///
    /// Agent context (role_name, role_abbreviation, etc.) comes from the
    /// embedded YAML frontmatter.
    ///
    /// Section content (output_format, max_findings, submit_finding) is
    /// rendered with the current context and injected as variables so that
    /// `agent.hbs` can reference them with `{{output_format}}` etc.
    pub fn render(&self, role: &str, vars: HashMap<String, serde_json::Value>) -> String {
        let entry = match self.agents.get(&role.to_uppercase()) {
            Some(e) => e,
            None => return format!("Unknown role: {}", role),
        };

        let mut ctx = serde_json::Map::new();
        vars.into_iter().for_each(|(k, v)| {
            ctx.insert(k, v);
        });

        let fields = AgentEntry::SERDE_FIELDS;
        for field in fields {
            ctx.insert(
                field.to_string(),
                match *field {
                    "role_name" => entry.role_name.clone(),
                    "role_abbreviation" => entry.role_abbreviation.clone(),
                    "role_domain" => entry.role_domain.clone(),
                    "role_anti_hallucination_rules" => entry
                        .role_anti_hallucination_rules
                        .clone()
                        .unwrap_or_default(),
                    "role_review_methodology" => {
                        entry.role_review_methodology.clone().unwrap_or_default()
                    }
                    "role_prompt" => entry.role_prompt.clone(),
                    _ => continue,
                }
                .into(),
            );
        }

        // Render section content and inject into context.
        // Sections themselves may contain template variables (e.g.
        // `max_findings.hbs` references `{{max_findings}}`, and
        // `output_format.hbs` references `{{role_abbreviation}}`),
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
                tracing::warn!("Failed to render agent template for '{}': {e}", role);
                format!("Error rendering prompt for {role}: {e}")
            })
    }

    /// List all known agent abbreviations.
    pub fn roles(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_library_loads() {
        let lib = PromptLibrary::new().expect("Should load from embedded");
        assert!(!lib.roles().is_empty());
        assert!(lib.get("SA").is_some() || lib.get("GEN").is_some());
    }
}
