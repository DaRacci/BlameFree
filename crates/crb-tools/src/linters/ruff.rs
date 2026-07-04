use crb_shared::finding::Finding;
use serde::Deserialize;

use crate::error::LinterError;

/// Internal JSON structure for ruff output.
#[derive(Debug, Deserialize)]
struct RuffJsonFinding {
    code: String,
    filename: String,
    location: RuffLocation,
    message: String,
}

#[derive(Debug, Deserialize)]
struct RuffLocation {
    column: u32,
    row: u32,
}

/// Parse ruff JSON output into [`Finding`] values.
pub fn parse_ruff_output(stdout: &str) -> Result<Vec<Finding>, LinterError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let items: Vec<RuffJsonFinding> =
        serde_json::from_str(trimmed).map_err(|e| LinterError::ParseFailed(e.to_string()))?;

    Ok(items
        .into_iter()
        .map(|f| Finding {
            file: Some(f.filename),
            line: Some(f.location.row),
            message: f.message,
            severity: "error".to_string(),
            rule_code: Some(f.code),
            severity_audited: false,
            severity_audit_reason: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
            ..Default::default()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ruff_output_empty() {
        let result = parse_ruff_output("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_ruff_output_whitespace() {
        let result = parse_ruff_output("  \n  ");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_ruff_output_valid() {
        let json = r#"[
            {"code": "F841", "filename": "src/main.py", "location": {"row": 10, "column": 5}, "message": "Local variable `x` is assigned but never used"},
            {"code": "E501", "filename": "src/utils.py", "location": {"row": 42, "column": 80}, "message": "Line too long"}
        ]"#;
        let result = parse_ruff_output(json);
        assert!(result.is_ok());
        let findings = result.unwrap();
        assert_eq!(findings.len(), 2);

        assert_eq!(findings[0].file.as_deref(), Some("src/main.py"));
        assert_eq!(findings[0].line, Some(10));
        assert_eq!(
            findings[0].message,
            "Local variable `x` is assigned but never used"
        );
        assert_eq!(findings[0].severity, "error");
        assert_eq!(findings[0].rule_code.as_deref(), Some("F841"));

        assert_eq!(findings[1].file.as_deref(), Some("src/utils.py"));
        assert_eq!(findings[1].line, Some(42));
        assert_eq!(findings[1].rule_code.as_deref(), Some("E501"));
    }

    #[test]
    fn test_parse_ruff_output_malformed() {
        let result = parse_ruff_output("not valid json");
        assert!(result.is_err());
        match result.unwrap_err() {
            LinterError::ParseFailed(_) => {} // expected
            other => panic!("expected ParseFailed, got {other:?}"),
        }
    }
}
