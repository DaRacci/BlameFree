//! Preamble formatting for agent system prompt injection.
//!
//! Builds a human-readable "Applicable Project Rules" section from matched
//! rules, suitable for prepending to a role-specific agent preamble.

use crate::Rule;

/// Build a formatted preamble string from a list of matched rules.
///
/// # Format
///
/// ```text
/// ## Applicable Project Rules
///
/// ### Rule Description
/// Rule body content...
///
/// ### Another Rule
/// Another rule body...
/// ```
///
/// If `matched` is empty, returns an empty string (so the caller can skip
/// preamble injection entirely).
///
/// Rules without a `description` skip the `### {description}` heading.
pub fn format_preamble(matched: &[&Rule]) -> String {
    if matched.is_empty() {
        return String::new();
    }

    let mut preamble = String::from("## Applicable Project Rules\n\n");
    for rule in matched {
        if let Some(desc) = &rule.description {
            preamble.push_str(&format!("### {}\n", desc));
        }
        preamble.push_str(&rule.body);
        preamble.push('\n');
        preamble.push('\n');
    }
    preamble
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn rule(description: Option<&str>, body: &str) -> Rule {
        Rule {
            description: description.map(String::from),
            globs: vec![],
            always_apply: true,
            body: body.to_string(),
            source_file: PathBuf::from("test.md"),
        }
    }

    #[test]
    fn test_format_preamble_with_descriptions() {
        let r1 = rule(Some("Python Standards"), "Use type hints for all public functions.");
        let r2 = rule(Some("Security"), "Always validate user input.");
        let matched = vec![&r1, &r2];

        let preamble = format_preamble(&matched);
        assert!(preamble.contains("## Applicable Project Rules"));
        assert!(preamble.contains("### Python Standards"));
        assert!(preamble.contains("Use type hints for all public functions."));
        assert!(preamble.contains("### Security"));
        assert!(preamble.contains("Always validate user input."));
    }

    #[test]
    fn test_format_preamble_empty_when_no_rules() {
        let preamble = format_preamble(&[]);
        assert!(preamble.is_empty());
    }

    #[test]
    fn test_format_preamble_rules_without_description_omit_heading() {
        let r = rule(None, "Some rule without a heading.");
        let matched = vec![&r];
        let preamble = format_preamble(&matched);
        assert!(preamble.contains("## Applicable Project Rules"));
        assert!(preamble.contains("Some rule without a heading."));
        assert!(!preamble.contains("###"));
    }

    #[test]
    fn test_format_preamble_mixed_descriptions() {
        let r1 = rule(Some("Has Description"), "Body with description.");
        let r2 = rule(None, "Body without description.");
        let matched = vec![&r1, &r2];
        let preamble = format_preamble(&matched);
        assert!(preamble.contains("### Has Description"));
        assert!(preamble.contains("Body with description."));
        assert!(preamble.contains("Body without description."));
    }

    #[test]
    fn test_format_preamble_ends_with_double_newline() {
        let r = rule(Some("Test"), "Content.");
        let matched = vec![&r];
        let preamble = format_preamble(&matched);
        assert!(preamble.ends_with("\n\n"));
    }
}
