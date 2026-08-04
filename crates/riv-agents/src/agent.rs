use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use mti::prelude::{MagicTypeId, NamespaceId, TypeIdPrefix, TypeIdSuffix};
use serde::Deserialize;
use serde_fields::SerdeField;

use crate::{AgentDetailsProvider, prompts::PromptLibrary};

/// A single agent entry parsed from a markdown manifest file.
#[derive(Debug, Clone, Deserialize, SerdeField, Default)]
pub struct AgentEntry {
    /// Unique identifier for this agent.
    ///
    /// Stable identifier derived from `role_abbreviation` for event/UI correlation.
    #[serde(default = "new_agent_id")]
    pub agent_id: MagicTypeId,

    /// Human-readable role name.
    pub role_name: String,

    /// Short abbreviation.
    ///
    /// This is also used as the unique identifier for this agent.
    /// There must not be any other agent with the same abbreviation.
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

fn new_agent_id() -> MagicTypeId {
    deterministic_agent_id("")
}

fn deterministic_agent_id(abbreviation: &str) -> MagicTypeId {
    let normalized = abbreviation.trim().to_uppercase();
    let prefix = TypeIdPrefix::try_from("agent").unwrap_or_default();
    let suffix = TypeIdSuffix::new_v5(NamespaceId::OID, normalized.as_bytes());
    MagicTypeId::new(prefix, suffix)
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

        entry.agent_id = deterministic_agent_id(&entry.role_abbreviation);

        Ok(entry)
    }
}

impl AgentDetailsProvider for AgentEntry {
    fn get_name(&self) -> &str {
        &self.role_name
    }

    fn get_prompt(&self, vars: HashMap<String, serde_json::Value>) -> String {
        PromptLibrary::get_instance().render(self, vars)
    }

    fn get_description(&self) -> &str {
        &self.role_domain
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
    use mti::prelude::{MagicTypeIdExt, V7};

    use super::*;

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
    fn test_deterministic_agent_id_from_abbreviation() {
        let first = AgentEntry::new(
            r#"---
role_name: Test One
role_abbreviation: test
role_domain: testing
---
Body one
"#,
        )
        .unwrap();
        let second = AgentEntry::new(
            r#"---
role_name: Test Two
role_abbreviation: TEST
role_domain: testing
---
Body two
"#,
        )
        .unwrap();
        let third = AgentEntry::new(
            r#"---
role_name: Other
role_abbreviation: OTHER
role_domain: testing
---
Body three
"#,
        )
        .unwrap();

        assert_eq!(first.agent_id, second.agent_id);
        assert_ne!(first.agent_id, third.agent_id);
    }

    #[test]
    fn test_frontmatter_agent_id_is_overridden() {
        let supplied_id = "agent".create_type_id::<V7>();
        let content = format!(
            r#"---
agent_id: {supplied_id}
role_name: Test
role_abbreviation: TEST
role_domain: testing
---
Body content here
"#
        );

        let entry = AgentEntry::new(&content).unwrap();

        assert_ne!(entry.agent_id, supplied_id);
        assert_eq!(entry.agent_id, deterministic_agent_id("TEST"));
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
