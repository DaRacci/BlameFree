//! Tests for `parse_agent_findings()`.
//!
//! Covers: valid JSON, field aliases, case normalisation, markdown fences,
//! empty arrays, invalid JSON, missing required fields.

use crb_agents::Finding;

/// Helper: call `parse_agent_findings` and unwrap the Ok result.
fn parse(response: &str) -> Vec<Finding> {
    crb_harness::parse_agent_findings(response).expect("parse_agent_findings should succeed")
}

// ---------------------------------------------------------------------------
// Valid JSON array matching Finding fields exactly
// ---------------------------------------------------------------------------

#[test]
fn valid_json_exact_fields() {
    let json = r#"[
        {"file": "src/main.rs", "line": 42, "message": "Unused variable", "severity": "High", "rule_code": "R001"},
        {"file": null, "line": null, "message": "Missing error handling", "severity": "Medium", "rule_code": null}
    ]"#;
    let findings = parse(json);
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].file.as_deref(), Some("src/main.rs"));
    assert_eq!(findings[0].line, Some(42));
    assert_eq!(findings[0].message, "Unused variable");
    assert_eq!(findings[0].severity, "High");
    assert_eq!(findings[0].rule_code.as_deref(), Some("R001"));
    assert_eq!(findings[1].message, "Missing error handling");
    assert_eq!(findings[1].severity, "Medium");
    assert!(findings[1].file.is_none());
    assert!(findings[1].line.is_none());
    assert!(findings[1].rule_code.is_none());
}

// ---------------------------------------------------------------------------
// JSON with field aliases (path→file, description→message, category→rule_code)
// ---------------------------------------------------------------------------

#[test]
fn field_alias_path_to_file() {
    let json = r#"[
        {"path": "src/lib.rs", "line": 10, "description": "Potential null deref", "severity": "Critical", "category": "SEC001"}
    ]"#;
    let findings = parse(json);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].file.as_deref(), Some("src/lib.rs"));
    assert_eq!(findings[0].message, "Potential null deref");
    assert_eq!(findings[0].rule_code.as_deref(), Some("SEC001"));
}

#[test]
fn field_alias_text_to_message() {
    let json = r#"[
        {"text": "Race condition on shared state", "severity": "High"}
    ]"#;
    let findings = parse(json);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].message, "Race condition on shared state");
}

#[test]
fn field_alias_component_to_file() {
    let json = r#"[
        {"component": "auth/mod.rs", "message": "Exposed secret", "severity": "Critical"}
    ]"#;
    let findings = parse(json);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].file.as_deref(), Some("auth/mod.rs"));
}

#[test]
fn field_alias_prefers_file_over_component() {
    // When both `file` and `component` are present, `file` wins.
    let json = r#"[
        {"file": "real.rs", "component": "alias.rs", "message": "test", "severity": "Low"}
    ]"#;
    let findings = parse(json);
    assert_eq!(findings[0].file.as_deref(), Some("real.rs"));
}

// ---------------------------------------------------------------------------
// Severity case normalisation
// ---------------------------------------------------------------------------

#[test]
fn severity_case_normalisation() {
    let json = r#"[
        {"message": "A", "severity": "high"},
        {"message": "B", "severity": "MEDIUM"},
        {"message": "C", "severity": "Low"},
        {"message": "D", "severity": "CRITICAL"},
        {"message": "E", "severity": "info"}
    ]"#;
    let findings = parse(json);
    assert_eq!(findings.len(), 5);
    assert_eq!(findings[0].severity, "High");
    assert_eq!(findings[1].severity, "Medium");
    assert_eq!(findings[2].severity, "Low");
    assert_eq!(findings[3].severity, "Critical");
    assert_eq!(findings[4].severity, "Info");
}

#[test]
fn severity_med_and_crit_abbreviations() {
    let json = r#"[
        {"message": "X", "severity": "med"},
        {"message": "Y", "severity": "crit"}
    ]"#;
    let findings = parse(json);
    assert_eq!(findings[0].severity, "Medium");
    assert_eq!(findings[1].severity, "Critical");
}

#[test]
fn severity_unknown_case_preserved() {
    // Unknown severity values are kept as-is
    let json = r#"[
        {"message": "Z", "severity": "Urgent"}
    ]"#;
    let findings = parse(json);
    assert_eq!(findings[0].severity, "Urgent");
}

// ---------------------------------------------------------------------------
// JSON wrapped in markdown fences
// ---------------------------------------------------------------------------

#[test]
fn markdown_fences_json() {
    let input = "Some text before\n```json\n[\n{\"file\": \"a.rs\", \"message\": \"Found bug\", \"severity\": \"Medium\"}\n]\n```\nAfter text";
    let findings = parse(input);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].message, "Found bug");
    assert_eq!(findings[0].severity, "Medium");
}

#[test]
fn markdown_fences_no_language() {
    let input = "```\n[{\"message\": \"Bare fence\", \"severity\": \"High\"}]\n```";
    let findings = parse(input);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].message, "Bare fence");
}

#[test]
fn markdown_fences_malformed_json_inside() {
    // If the fence contains invalid JSON, it should be skipped
    let input = "```json\n{this is not json}\n```";
    let findings = parse(input);
    assert!(findings.is_empty());
}

// ---------------------------------------------------------------------------
// Empty array
// ---------------------------------------------------------------------------

#[test]
fn empty_array() {
    let findings = parse("[]");
    assert!(findings.is_empty());
}

// ---------------------------------------------------------------------------
// Invalid JSON (should not panic, return empty vec)
// ---------------------------------------------------------------------------

#[test]
fn invalid_json_no_panic() {
    let findings = parse("this is not json at all");
    assert!(findings.is_empty());
}

#[test]
fn invalid_json_partial_object() {
    let findings = parse(r#"{"unclosed": "object"#);
    assert!(findings.is_empty());
}

#[test]
fn invalid_json_non_object_elements() {
    // Array of non-object values
    let findings = parse(r#"["string", 42, null]"#);
    assert!(findings.is_empty());
}

// ---------------------------------------------------------------------------
// Missing required field "message" (should fail gracefully)
// ---------------------------------------------------------------------------

#[test]
fn missing_message_field() {
    let json = r#"[
        {"file": "a.rs", "severity": "High"}
    ]"#;
    // The Finding struct requires message (String, not Option), so this should
    // fail to deserialise and return an empty vec.
    let findings = parse(json);
    assert!(findings.is_empty());
}

#[test]
fn missing_severity_field() {
    let json = r#"[
        {"message": "Something wrong"}
    ]"#;
    // severity is String (not Option), so missing severity should fail.
    let findings = parse(json);
    assert!(findings.is_empty());
}

// ---------------------------------------------------------------------------
// Large finding set
// ---------------------------------------------------------------------------

#[test]
fn ten_findings() {
    let items: Vec<String> = (0..10)
        .map(|i| {
            format!(
                r#"{{"message": "Finding {}", "severity": "Low", "file": "f{}.rs", "line": {}}}"#,
                i, i, i * 10
            )
        })
        .collect();
    let json = format!("[{}]", items.join(","));
    let findings = parse(&json);
    assert_eq!(findings.len(), 10);
    assert_eq!(findings[0].message, "Finding 0");
    assert_eq!(findings[9].message, "Finding 9");
}
