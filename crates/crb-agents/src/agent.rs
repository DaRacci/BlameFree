use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use serde_fields::SerdeField;

/// A single agent entry parsed from a markdown manifest file.
#[derive(Debug, Clone, Deserialize, SerdeField, Default)]
pub struct AgentEntry {
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
