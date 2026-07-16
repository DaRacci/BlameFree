use crb_shared::{
    finding::{ConfidenceLevel, Finding},
    severity::Severity,
};
use tracing::warn;

use crate::error::LinterError;

const LINTER_IDENTIFIER: &str = "linter/govet";

/// Parse `go vet` text output into [`Finding`] values.
///
/// `go vet` outputs lines in the format:
/// ```text
/// ./src/main.go:25:2: unreachable code
/// ```
pub fn parse_govet_output(stdout: &str) -> Result<Vec<Finding>, LinterError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut findings = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse: ./path/file.go:line:col: message
        // Or: ./path/file.go:line: message
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() < 3 {
            warn!("Unrecognized go vet output line: {}", line);
            continue;
        }

        let file = parts[0].to_string();
        let line_num: Option<u32> = parts[1].parse().ok();

        // Message is everything after the last colon-separated segment.
        let message = parts[parts.len().saturating_sub(1)..]
            .join(":")
            .trim()
            .to_string();

        findings.push(Finding {
            file: Some(file),
            line: line_num,
            message,
            severity: Severity::Low,
            rule_code: None,
            severity_audited: false,
            severity_audit_reason: None,
            evidence: Some(line.to_string()),
            // TODO: get command executed from the linter context and include it here
            path_trace: None,
            confidence: Some(ConfidenceLevel::Confirmed),
            found_by: Some(LINTER_IDENTIFIER.to_string()),
            ..Default::default()
        });
    }

    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_govet_output_empty() {
        let result = parse_govet_output("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_govet_output_valid() {
        let text = "./src/main.go:25:2: unreachable code\n./src/util.go:42:6: X is unused\n";
        let result = parse_govet_output(text);
        assert!(result.is_ok());
        let findings = result.unwrap();
        assert_eq!(findings.len(), 2);

        assert_eq!(findings[0].file.as_deref(), Some("./src/main.go"));
        assert_eq!(findings[0].line, Some(25));
        assert_eq!(findings[0].message, "unreachable code");
        assert!(findings[0].rule_code.is_none());

        assert_eq!(findings[1].file.as_deref(), Some("./src/util.go"));
        assert_eq!(findings[1].line, Some(42));
        assert_eq!(findings[1].message, "X is unused");
    }

    #[test]
    fn test_parse_govet_output_no_colon_format() {
        // Some go vet output may not have colons in the expected format
        let text = "./src/main.go:25: unreachable code";
        let result = parse_govet_output(text);
        assert!(result.is_ok());
        let findings = result.unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file.as_deref(), Some("./src/main.go"));
        assert_eq!(findings[0].line, Some(25));
        assert_eq!(findings[0].message, "unreachable code");
    }

    #[test]
    fn test_parse_govet_output_multi_colon_message() {
        let output = "./src/main.go:25:2: found: unresolved identifier: Foo";
        let findings = parse_govet_output(output).unwrap();
        insta::assert_debug_snapshot!(findings.len());
        insta::assert_snapshot!(findings[0].message.as_str());
    }

    #[test]
    fn test_parse_govet_output_unrecognized_format_line() {
        let output = "./src/main.go:25:2: unreachable code\nwarning: this text is not in expected format\n./src/util.go:42:6: X is unused";
        let findings = parse_govet_output(output).unwrap();
        insta::assert_debug_snapshot!(findings.len());
        insta::assert_snapshot!(findings[1].message.as_str());
    }
}
