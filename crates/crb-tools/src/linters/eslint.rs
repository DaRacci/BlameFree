use crb_shared::{
    finding::{ConfidenceLevel, Finding},
    severity::Severity,
};
use serde::Deserialize;

use crate::error::LinterError;

/// Internal JSON structure for ESLint output.
///
/// ESLint outputs JSON in the format:
/// ```json
/// [{"filePath": "...", "messages": [{"ruleId": "...", "severity": 2, "line": 15, "column": 3, "message": "..."}]}]
/// ```
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EslintFileResult {
    file_path: String,
    messages: Vec<EslintMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EslintMessage {
    rule_id: Option<String>,
    severity: i32,
    line: u32,
    #[allow(unused)]
    column: u32,
    message: String,
}

/// Parse ESLint JSON output into [`Finding`] values.
pub fn parse_eslint_output(stdout: &str) -> Result<Vec<Finding>, LinterError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let items: Vec<EslintFileResult> =
        serde_json::from_str(trimmed).map_err(|e| LinterError::ParseFailed(e.to_string()))?;

    let mut findings = Vec::new();
    for file_result in items {
        for msg in file_result.messages {
            let severity = match msg.severity {
                2 => Severity::Critical,
                1 => Severity::Low,
                _ => Severity::Info,
            };

            findings.push(Finding {
                file: Some(file_result.file_path.clone()),
                line: Some(msg.line),
                message: msg.message,
                severity,
                rule_code: msg.rule_id,
                severity_audited: true,
                severity_audit_reason: Some("Determined by linter severity level".to_string()),
                evidence: Some(trimmed.to_string()),
                path_trace: None,
                confidence: Some(ConfidenceLevel::Confirmed),
                found_by: Some("linter/eslint".to_string()),
                ..Default::default()
            });
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_eslint_output_empty() {
        let result = parse_eslint_output("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_eslint_output_valid() {
        let json = r#"[
            {
                "filePath": "/repo/src/app.js",
                "messages": [
                    {
                        "ruleId": "no-unused-vars",
                        "severity": 2,
                        "line": 15,
                        "column": 3,
                        "message": "'x' is assigned but never used"
                    },
                    {
                        "ruleId": "no-console",
                        "severity": 1,
                        "line": 20,
                        "column": 1,
                        "message": "Unexpected console statement"
                    }
                ]
            }
        ]"#;
        let result = parse_eslint_output(json);
        assert!(result.is_ok());
        let findings = result.unwrap();
        assert_eq!(findings.len(), 2);

        assert_eq!(findings[0].file.as_deref(), Some("/repo/src/app.js"));
        assert_eq!(findings[0].line, Some(15));
        assert_eq!(findings[0].message, "'x' is assigned but never used");
        assert_eq!(findings[0].rule_code.as_deref(), Some("no-unused-vars"));

        assert_eq!(findings[1].rule_code.as_deref(), Some("no-console"));
    }

    #[test]
    fn test_parse_eslint_output_malformed() {
        let result = parse_eslint_output("{bad json");
        assert!(result.is_err());
        match result.unwrap_err() {
            LinterError::ParseFailed(_) => {}
            other => panic!("expected ParseFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_eslint_output_severity_edge_cases() {
        let json = r#"[
            {
                "filePath": "/repo/src/app.js",
                "messages": [
                    {"ruleId": "no-console", "severity": 0, "line": 1, "column": 1, "message": "off but present"},
                    {"ruleId": "no-eval", "severity": 3, "line": 2, "column": 1, "message": "unknown severity"}
                ]
            }
        ]"#;
        let findings = parse_eslint_output(json).unwrap();
        insta::assert_debug_snapshot!(findings.len());
        insta::assert_debug_snapshot!(findings[0].severity);
        insta::assert_debug_snapshot!(findings[1].severity);
    }

    #[test]
    fn test_parse_eslint_output_no_messages() {
        let json = r#"[
            {
                "filePath": "/repo/src/app.js",
                "messages": []
            }
        ]"#;
        let findings = parse_eslint_output(json).unwrap();
        insta::assert_debug_snapshot!(findings.len());
    }
}
