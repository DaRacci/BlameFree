//! Embedded prompt library — prompts compiled into the binary.
//!
//! Agent `.md` files have YAML frontmatter that is parsed and stripped.
//! All rendering goes through `agent.hbs`, never raw markdown.

use handlebars::Handlebars;
use include_dir::{include_dir, Dir};
use serde::Deserialize;
use std::collections::HashMap;

/// Embedded prompts directory (compiled into binary).
static PROMPTS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../prompts");

/// Parsed agent configuration from YAML frontmatter.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AgentConfig {
    #[serde(default)]
    pub role_name: String,
    #[serde(default)]
    pub role_abbreviation: String,
    #[serde(default)]
    pub role_domain: String,
    #[serde(default)]
    pub role_anti_hallucination_rules: String,
    #[serde(default)]
    pub role_review_methodology: String,
    #[serde(default)]
    pub generalist_agent: bool,
    #[serde(default)]
    pub incompatible_with_roles: Vec<String>,
}

/// An agent entry: parsed config + the markdown body (role_prompt).
#[derive(Debug, Clone)]
pub struct AgentEntry {
    pub config: AgentConfig,
    pub role_prompt: String,
}

/// Embedded prompt library
#[derive(Clone)]
pub struct PromptLibrary {
    agents: HashMap<String, AgentEntry>,
    handlebars: Handlebars<'static>,
}

impl PromptLibrary {
    /// Initialise from embedded data. Returns error if agent.hbs is missing
    /// or no agents are found.
    pub fn new() -> Result<Self, String> {
        let agent_template = PROMPTS_DIR
            .get_file("builtin/handlebars/agent.hbs")
            .ok_or("agent.hbs not found in embedded prompts")?
            .contents_utf8()
            .ok_or("agent.hbs is not valid UTF-8")?
            .to_string();

        let mut sections: HashMap<String, String> = HashMap::new();
        if let Some(sections_dir) = PROMPTS_DIR.get_dir("sections") {
            for entry in sections_dir.files() {
                let name = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                let content = entry.contents_utf8().unwrap_or("").to_string();
                // Map submit_findings → submit_finding for template compat
                let key = if name == "submit_findings" {
                    "submit_finding"
                } else {
                    name
                };
                sections.insert(key.to_string(), content);
            }
        }

        let mut agents: HashMap<String, AgentEntry> = HashMap::new();
        if let Some(agents_dir) = PROMPTS_DIR.get_dir("agents") {
            for file in agents_dir.files() {
                if file.path().extension().map_or(true, |e| e != "md") {
                    continue;
                }
                let content = file.contents_utf8().unwrap_or("");
                if let Some((yaml_str, body)) = split_frontmatter(content) {
                    let config: AgentConfig = serde_yaml::from_str(yaml_str).unwrap_or_default();
                    if config.role_abbreviation.is_empty() {
                        continue;
                    }
                    let clean_body = body.trim().to_string();
                    agents.insert(
                        config.role_abbreviation.to_uppercase(),
                        AgentEntry {
                            config,
                            role_prompt: clean_body,
                        },
                    );
                }
            }
        }

        if agents.is_empty() {
            return Err("No agents found in embedded prompts".into());
        }

        let mut hb = Handlebars::new();
        hb.set_strict_mode(false);
        // Register agent.hbs as "agent"
        hb.register_template_string("agent", &agent_template)
            .map_err(|e| format!("Failed to register agent template: {e}"))?;
        // Register each section as a partial
        for (name, content) in &sections {
            hb.register_partial(name, content)
                .map_err(|e| format!("Failed to register partial '{name}': {e}"))?;
        }

        Ok(Self {
            agents,
            handlebars: hb,
        })
    }

    /// Get the raw markdown body for a role with the YAML stripped.
    pub fn get(&self, role: &str) -> Option<&str> {
        self.agents
            .get(&role.to_uppercase())
            .map(|e| e.role_prompt.as_str())
    }

    /// Get the agent config for a role.
    pub fn config(&self, role: &str) -> Option<&AgentConfig> {
        self.agents.get(&role.to_uppercase()).map(|e| &e.config)
    }

    /// Render a role's prompt through agent.hbs.
    ///
    /// `vars` carries runtime variables like `diff`, `file_list`, `language`.
    /// Agent context (role_name, role_abbreviation, etc.) comes from the
    /// embedded YAML frontmatter.
    pub fn render(&self, role: &str, vars: &HashMap<String, serde_json::Value>) -> String {
        let entry = match self.agents.get(&role.to_uppercase()) {
            Some(e) => e,
            None => return format!("Unknown role: {}", role),
        };

        let mut ctx = serde_json::Map::new();
        ctx.insert("role_name".into(), entry.config.role_name.clone().into());
        ctx.insert(
            "role_abbreviation".into(),
            entry.config.role_abbreviation.clone().into(),
        );
        ctx.insert(
            "role_domain".into(),
            entry.config.role_domain.clone().into(),
        );
        ctx.insert(
            "role_anti_hallucination_rules".into(),
            entry.config.role_anti_hallucination_rules.clone().into(),
        );
        ctx.insert(
            "role_review_methodology".into(),
            entry.config.role_review_methodology.clone().into(),
        );
        ctx.insert("role_prompt".into(), entry.role_prompt.clone().into());

        for (k, v) in vars {
            ctx.insert(k.clone(), v.clone());
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

/// Split YAML frontmatter from a .md file.
/// Returns Some((yaml_str, body)) if the file starts with ---,
/// or None if no frontmatter found.
fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.trim();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    let yaml = rest[..end].trim();
    let body = rest[end + 4..].trim();
    Some((yaml, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter() {
        let content = "---\nkey: value\n---\n\nbody text";
        let (yaml, body) = split_frontmatter(content).unwrap();
        assert_eq!(yaml, "key: value");
        assert_eq!(body, "body text");
    }

    #[test]
    fn test_no_frontmatter() {
        assert!(split_frontmatter("plain text").is_none());
    }

    #[test]
    fn test_prompt_library_loads() {
        let lib = PromptLibrary::new().expect("Should load from embedded");
        assert!(lib.roles().len() >= 1);
        assert!(lib.get("SA").is_some() || lib.get("GEN").is_some());
    }
}
