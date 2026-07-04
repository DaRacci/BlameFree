//! Agent manifest loading from `prompts/agents/*.md`.
//!
//! Each markdown file has YAML frontmatter between `---` markers.
//! Exactly one file must declare `generalist_agent: true`.
//! The markdown body after the frontmatter becomes `role_prompt`.

use serde::Deserialize;
use serde_fields::SerdeField;
use std::collections::{HashMap, HashSet};
use std::path::Path;

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

    /// Roles this agent is incompatible with (e.g. generalist
    /// is incompatible with individual specialist agents).
    #[serde(default)]
    pub incompatible_with_roles: Vec<String>,

    /// The markdown body after YAML frontmatter (filled after parsing).
    #[serde(skip)]
    pub role_prompt: String,
}

/// Holds all parsed agent manifests from `prompts/agents/*.md`.
#[derive(Debug, Clone)]
pub struct AgentManifest {
    /// Map from abbreviation to entry.
    agents: HashMap<String, AgentEntry>,

    /// The abbreviation of the generalist agent.
    generalist_abbreviation: Option<String>,
}

/// Split YAML frontmatter from a `.md` file.
/// Returns `Some((yaml_str, body))` if the file starts with `---`,
/// or `None` if no frontmatter is found.
pub(crate) fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
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

impl AgentManifest {
    /// Load all agent manifests from a directory of `.md` files.
    ///
    /// Each file must have YAML frontmatter delimited by `---`.
    /// Exactly one file must have `generalist_agent: true`.
    ///
    /// Returns an error if:
    /// - The directory cannot be read.
    /// - Any `.md` file cannot be read or parsed.
    /// - Zero or more than one file has `generalist_agent: true`.
    /// - Duplicate `role_abbreviation` values are found.
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        let mut agents: HashMap<String, AgentEntry> = HashMap::new();
        let mut generalist_count = 0u32;
        let mut generalist_abbreviation: Option<String> = None;

        let mut seen_abbreviations: HashSet<String> = HashSet::new();

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md") {
                let content = std::fs::read_to_string(&path)?;
                let entry = Self::parse_frontmatter(&content, &path)?;

                let abbr = entry.role_abbreviation.to_uppercase();
                if !seen_abbreviations.insert(abbr.clone()) {
                    anyhow::bail!(
                        "Duplicate role_abbreviation '{}' in {}",
                        abbr,
                        path.display()
                    );
                }

                if entry.generalist_agent {
                    generalist_count += 1;
                    generalist_abbreviation = Some(abbr.clone());
                }

                agents.insert(abbr, entry);
            }
        }

        match generalist_count {
            0 => anyhow::bail!(
                "No agent with `generalist_agent: true` found in {}",
                dir.display()
            ),
            1 => {} // OK
            n => anyhow::bail!(
                "Expected exactly one `generalist_agent: true`, found {} in {}",
                n,
                dir.display()
            ),
        }

        Ok(Self {
            agents,
            generalist_abbreviation,
        })
    }

    /// Parse YAML frontmatter and markdown body from a `.md` file.
    fn parse_frontmatter(content: &str, path: &Path) -> anyhow::Result<AgentEntry> {
        let (yaml_str, body) = split_frontmatter(content).ok_or_else(|| {
            anyhow::anyhow!(
                "Agent file {} does not start with YAML frontmatter (`---`)",
                path.display()
            )
        })?;

        // Parse YAML frontmatter
        let mut entry: AgentEntry = serde_yaml::from_str(yaml_str).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse YAML frontmatter in {}: {}",
                path.display(),
                e
            )
        })?;

        let role_prompt = body.to_string();
        entry.role_prompt = role_prompt.clone();

        if entry.role_abbreviation.is_empty() {
            anyhow::bail!(
                "Agent file {} has empty `role_abbreviation`",
                path.display()
            );
        }

        Ok(entry)
    }

    /// Get an agent entry by abbreviation (case-insensitive).
    ///
    /// Returns `None` if no agent with that abbreviation is registered.
    pub fn get(&self, abbreviation: &str) -> Option<&AgentEntry> {
        self.agents.get(&abbreviation.to_uppercase())
    }

    /// Get the generalist agent entry, if any.
    pub fn generalist(&self) -> Option<&AgentEntry> {
        self.generalist_abbreviation
            .as_ref()
            .and_then(|a| self.agents.get(a))
    }

    /// Get the abbreviation of the generalist agent.
    pub fn generalist_abbreviation(&self) -> Option<&str> {
        self.generalist_abbreviation.as_deref()
    }

    /// Return all registered role abbreviations.
    pub fn all_abbreviations(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.agents.keys().map(|k| k.as_str()).collect();
        keys.sort();
        keys
    }

    /// Return the list of roles incompatible with the given abbreviation.
    pub fn incompatible_with(&self, abbreviation: &str) -> Vec<&str> {
        self.get(abbreviation)
            .map(|entry| {
                entry
                    .incompatible_with_roles
                    .iter()
                    .map(|r| r.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if the given abbreviation is a valid agent.
    pub fn has(&self, abbreviation: &str) -> bool {
        self.agents.contains_key(&abbreviation.to_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_agents_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../prompts/agents")
    }

    #[test]
    fn test_load_manifest_from_real_files() {
        let dir = test_agents_dir();
        if !dir.exists() {
            eprintln!("Skipping: agents directory not found at {:?}", dir);
            return;
        }
        let manifest = AgentManifest::load_from_dir(&dir).unwrap();
        assert!(manifest.has("SA"));
        assert!(manifest.has("CL"));
        assert!(manifest.has("ARCH"));
        assert!(manifest.has("SEC"));
        assert!(manifest.has("GEN"));

        let gen = manifest.generalist().unwrap();
        assert_eq!(gen.role_abbreviation, "GEN");
        assert!(gen.generalist_agent);
    }

    #[test]
    fn test_load_manifest_abbreviations() {
        let dir = test_agents_dir();
        if !dir.exists() {
            eprintln!("Skipping: agents directory not found at {:?}", dir);
            return;
        }
        let manifest = AgentManifest::load_from_dir(&dir).unwrap();
        let abbrs = manifest.all_abbreviations();
        assert!(abbrs.contains(&"SA"));
        assert!(abbrs.contains(&"CL"));
        assert!(abbrs.contains(&"ARCH"));
        assert!(abbrs.contains(&"SEC"));
        assert!(abbrs.contains(&"GEN"));
    }

    #[test]
    fn test_get_sa_entry() {
        let dir = test_agents_dir();
        if !dir.exists() {
            eprintln!("Skipping: agents directory not found at {:?}", dir);
            return;
        }
        let manifest = AgentManifest::load_from_dir(&dir).unwrap();
        let sa = manifest.get("SA").unwrap();
        assert_eq!(sa.role_name, "Static Analysis");
        assert_eq!(sa.role_abbreviation, "SA");
        assert!(!sa.role_prompt.is_empty());
    }

    #[test]
    fn test_get_gen_entry() {
        let dir = test_agents_dir();
        if !dir.exists() {
            eprintln!("Skipping: agents directory not found at {:?}", dir);
            return;
        }
        let manifest = AgentManifest::load_from_dir(&dir).unwrap();
        let gen = manifest.generalist().unwrap();
        assert_eq!(gen.role_name, "General");
        assert_eq!(gen.role_abbreviation, "GEN");
        assert!(gen.generalist_agent);
        assert_eq!(gen.incompatible_with_roles, vec!["SEC", "SA", "CL", "ARCH"]);
    }

    #[test]
    fn test_incompatible_with() {
        let dir = test_agents_dir();
        if !dir.exists() {
            eprintln!("Skipping: agents directory not found at {:?}", dir);
            return;
        }
        let manifest = AgentManifest::load_from_dir(&dir).unwrap();
        let incompatible = manifest.incompatible_with("GEN");
        assert_eq!(incompatible.len(), 4);
        assert!(incompatible.contains(&"SEC"));
    }

    #[test]
    fn test_get_case_insensitive() {
        let dir = test_agents_dir();
        if !dir.exists() {
            eprintln!("Skipping: agents directory not found at {:?}", dir);
            return;
        }
        let manifest = AgentManifest::load_from_dir(&dir).unwrap();
        assert!(manifest.get("sa").is_some());
        assert!(manifest.get("Sa").is_some());
        assert!(manifest.get("gen").is_some());
    }

    #[test]
    fn test_get_unknown_role() {
        let dir = test_agents_dir();
        if !dir.exists() {
            eprintln!("Skipping: agents directory not found at {:?}", dir);
            return;
        }
        let manifest = AgentManifest::load_from_dir(&dir).unwrap();
        assert!(manifest.get("UNKNOWN").is_none());
    }

    #[test]
    fn test_frontmatter_parsing() {
        // Test parsing of a minimal frontmatter
        let content = r#"---
role_name: Test
role_abbreviation: TEST
role_domain: testing
---
Body content here
"#;
        let entry = AgentManifest::parse_frontmatter(content, &PathBuf::from("test.md")).unwrap();
        assert_eq!(entry.role_name, "Test");
        assert_eq!(entry.role_abbreviation, "TEST");
        assert_eq!(entry.role_prompt, "Body content here");
        assert!(!entry.generalist_agent);
    }

    #[test]
    fn test_invalid_file_no_frontmatter() {
        let content = "Just a plain file without frontmatter";
        let result = AgentManifest::parse_frontmatter(content, &PathBuf::from("bad.md"));
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
        let result = AgentManifest::parse_frontmatter(content, &PathBuf::from("bad.md"));
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_detects_no_generalist_error() {
        let dir = std::env::temp_dir().join("manifest_test_no_gen");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("sa.md"),
            r#"---
role_name: Static Analysis
role_abbreviation: SA
role_domain: testing
---
Body"#,
        )
        .unwrap();

        let result = AgentManifest::load_from_dir(&dir);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No agent with `generalist_agent: true`"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_manifest_detects_duplicate_abbreviations() {
        let dir = std::env::temp_dir().join("manifest_test_dup");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("one.md"),
            r#"---
role_name: One
role_abbreviation: DUP
role_domain: test
generalist_agent: true
---
Body"#,
        )
        .unwrap();

        std::fs::write(
            dir.join("two.md"),
            r#"---
role_name: Two
role_abbreviation: DUP
role_domain: test
---
Body"#,
        )
        .unwrap();

        let result = AgentManifest::load_from_dir(&dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate"));

        let _ = std::fs::remove_dir_all(&dir);
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
