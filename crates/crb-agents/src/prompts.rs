use anyhow::{Result, anyhow, bail};
use handlebars::Handlebars;
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use serde_fields::SerdeField;
use std::collections::HashMap;

use crate::templates;

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
    #[deprecated = "Use PromptLibrary::get_instance() instead."]
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

    /// Render a role's prompt through the agent template.
    ///
    /// `vars` provides runtime variables like `diff`, `file_list`, `language`
    ///
    /// Agent context comes from the embedded YAML frontmatter.
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
                tracing::warn!("Failed to render agent template for '{}': {e}", role);
                format!("Error rendering prompt for {role}: {e}")
            })
    }

    /// List all known agent entries.
    pub fn roles(&self) -> Vec<&AgentEntry> {
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

/// A single agent entry parsed from a markdown manifest file.
#[derive(Debug, Clone, Deserialize, SerdeField, Default)]
pub struct AgentEntry {
    /// Human-readable role name.
    pub role_name: String,

    /// Short abbreviation.
    pub role_abbreviation: String,

    /// Description of the role's domain.
    pub role_domain: String,

    /// Additional rules to append to the Anti-hallucination rules section.
    #[serde(default)]
    pub role_anti_hallucination_rules: Option<String>,

    /// Review methodology text.
    #[serde(default)]
    pub role_review_methodology: Option<String>,

    /// Whether this agent is the generalist.
    #[serde(default)]
    pub generalist_agent: bool,

    /// Roles this agent is incompatible with.
    #[serde(default)]
    pub incompatible_with_roles: Vec<String>,

    /// The markdown body after YAML frontmatter.
    ///
    /// This section is only filled after parsing the markdown file and is not part of the YAML frontmatter.
    #[serde(skip)]
    pub role_prompt: String,
}

impl AgentEntry {
    /// Parse YAML frontmatter and markdown body from a `.md` file.
    pub(crate) fn new(content: &str) -> Result<AgentEntry> {
        let (yaml_str, body) = split_frontmatter(content)
            .ok_or_else(|| anyhow!("Agent does not start with YAML frontmatter (`---`)",))?;

        let mut entry: AgentEntry = serde_yaml::from_str(yaml_str)
            .map_err(|e| anyhow!("Failed to parse YAML frontmatter: {e}"))?;

        let role_prompt = body.trim().to_string();
        entry.role_prompt = role_prompt.clone();

        if entry.role_abbreviation.is_empty() {
            bail!("Agent has empty `role_abbreviation`",);
        }

        Ok(entry)
    }
}

/// Split YAML frontmatter from a `.md` file.
///
/// Returns `Some((yaml_str, body))` if the file starts with `---`,
/// or `None` if no frontmatter is found.
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
    fn test_prompt_library_loads() {
        let lib = PromptLibrary::get_instance();
        assert!(!lib.roles().is_empty());
        assert!(lib.get("SA").is_some() || lib.get("GEN").is_some());
    }

    #[test]
    fn test_frontmatter_parsing() {
        let content = r#"---
role_name: Test
role_abbreviation: TEST
role_domain: testing
---
Body content here
"#;
        let entry = AgentEntry::new(content).unwrap();
        assert_eq!(entry.role_name, "Test");
        assert_eq!(entry.role_abbreviation, "TEST");
        assert_eq!(entry.role_prompt, "Body content here");
        assert!(!entry.generalist_agent);
    }

    #[test]
    fn test_invalid_file_no_frontmatter() {
        let content = "Just a plain file without frontmatter";
        let result = AgentEntry::new(content);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not start with YAML frontmatter"));
    }

    #[test]
    fn test_invalid_yaml() {
        let content = r#"---
role_name: Test
role_abbreviation:
  - invalid: yaml
---
body
"#;
        let result = AgentEntry::new(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_split_frontmatter_valid() {
        let content = "---\nkey: value\n---\n\nbody text";
        let (yaml, body) = split_frontmatter(content).unwrap();
        assert_eq!(yaml, "key: value");
        assert_eq!(body, "body text");
    }

    #[test]
    fn test_split_frontmatter_no_frontmatter() {
        assert!(split_frontmatter("plain text").is_none());
    }
}
