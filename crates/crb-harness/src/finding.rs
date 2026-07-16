use std::collections::HashSet;
use tracing::info;

use crb_auditor::apply_severity_auditor;
use crb_shared::{deduplicate::semantic_dedup, finding::Finding};

const MAX_FINDINGS: usize = 20;

/// Post-process findings through aggregator dedup and auditor severity checks.
pub fn post_process_findings(findings: &[Finding]) -> Vec<Finding> {
    if findings.is_empty() {
        return findings.to_vec();
    }

    let mut findings = semantic_dedup(findings.to_vec());
    apply_severity_auditor(&mut findings);
    let capped = {
        let max = MAX_FINDINGS;
        if findings.len() > max {
            info!("capping {} findings to {} candidates", findings.len(), max);
            findings.into_iter().take(max).collect()
        } else {
            findings
        }
    };

    capped
}

/// Deduplicate a list of findings by (file, line) pairs.
///
/// When two findings share the same file path and line number, only the first occurrence is kept.
/// This avoids double-counting findings that multiple agents or chunks produced for the same location.
///
/// # Ordering
///
/// The deduplication is stable: the first occurrence of each (file, line) pair
/// is retained, and subsequent duplicates are dropped.
pub fn deduplicate_findings(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen: HashSet<(String, u32)> = HashSet::new();
    let mut result = Vec::with_capacity(findings.len());

    for f in findings {
        let key = (f.file.clone().unwrap_or_default(), f.line.unwrap_or(0));
        if seen.insert(key) {
            result.push(f);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_finding(file: Option<&str>, line: Option<u32>, msg: &str) -> Finding {
        Finding {
            file: file.map(String::from),
            line,
            message: msg.to_string(),
            severity: crb_shared::severity::Severity::Medium,
            evidence: None,
            rule_code: None,
            severity_audited: false,
            severity_audit_reason: None,
            path_trace: None,
            confidence: None,
            found_by: None,
            agent_count: None,
            cross_validated: false,
            cross_validated_by: None,
            merged_from: None,
        }
    }

    #[test]
    fn test_deduplicate_findings_no_duplicates() {
        let findings = vec![
            make_finding(Some("src/main.rs"), Some(10), "first"),
            make_finding(Some("src/main.rs"), Some(20), "second"),
            make_finding(Some("src/lib.rs"), Some(5), "third"),
        ];
        let deduped = deduplicate_findings(findings);
        insta::assert_debug_snapshot!(deduped, @r#"
        [
            Finding {
                file: Some(
                    "src/main.rs",
                ),
                line: Some(
                    10,
                ),
                message: "first",
                severity: Medium,
                rule_code: None,
                severity_audited: false,
                severity_audit_reason: None,
                evidence: None,
                path_trace: None,
                confidence: None,
                found_by: None,
                agent_count: None,
                cross_validated: false,
                cross_validated_by: None,
                merged_from: None,
            },
            Finding {
                file: Some(
                    "src/main.rs",
                ),
                line: Some(
                    20,
                ),
                message: "second",
                severity: Medium,
                rule_code: None,
                severity_audited: false,
                severity_audit_reason: None,
                evidence: None,
                path_trace: None,
                confidence: None,
                found_by: None,
                agent_count: None,
                cross_validated: false,
                cross_validated_by: None,
                merged_from: None,
            },
            Finding {
                file: Some(
                    "src/lib.rs",
                ),
                line: Some(
                    5,
                ),
                message: "third",
                severity: Medium,
                rule_code: None,
                severity_audited: false,
                severity_audit_reason: None,
                evidence: None,
                path_trace: None,
                confidence: None,
                found_by: None,
                agent_count: None,
                cross_validated: false,
                cross_validated_by: None,
                merged_from: None,
            },
        ]
        "#);
    }

    #[test]
    fn test_deduplicate_findings_with_duplicates() {
        let findings = vec![
            make_finding(Some("src/main.rs"), Some(10), "first"),
            make_finding(Some("src/main.rs"), Some(10), "duplicate"),
            make_finding(Some("src/lib.rs"), Some(5), "unique"),
        ];
        let deduped = deduplicate_findings(findings);
        insta::assert_debug_snapshot!(deduped, @r#"
        [
            Finding {
                file: Some(
                    "src/main.rs",
                ),
                line: Some(
                    10,
                ),
                message: "first",
                severity: Medium,
                rule_code: None,
                severity_audited: false,
                severity_audit_reason: None,
                evidence: None,
                path_trace: None,
                confidence: None,
                found_by: None,
                agent_count: None,
                cross_validated: false,
                cross_validated_by: None,
                merged_from: None,
            },
            Finding {
                file: Some(
                    "src/lib.rs",
                ),
                line: Some(
                    5,
                ),
                message: "unique",
                severity: Medium,
                rule_code: None,
                severity_audited: false,
                severity_audit_reason: None,
                evidence: None,
                path_trace: None,
                confidence: None,
                found_by: None,
                agent_count: None,
                cross_validated: false,
                cross_validated_by: None,
                merged_from: None,
            },
        ]
        "#);
    }

    #[test]
    fn test_deduplicate_findings_empty() {
        let findings: Vec<Finding> = vec![];
        let deduped = deduplicate_findings(findings);
        assert!(deduped.is_empty());
    }

    #[test]
    fn test_deduplicate_findings_different_files() {
        // Same line, different file should NOT deduplicate
        let findings = vec![
            make_finding(Some("src/main.rs"), Some(10), "in main"),
            make_finding(Some("src/lib.rs"), Some(10), "in lib"),
        ];
        let deduped = deduplicate_findings(findings);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_deduplicate_findings_no_file_no_line() {
        let findings = vec![
            make_finding(None, None, "no location"),
            make_finding(None, None, "also no location"),
        ];
        let deduped = deduplicate_findings(findings);
        // Both have (file="", line=0) as key, so second is deduped
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].message, "no location");
    }

    #[test]
    fn test_deduplicate_findings_stable_order() {
        // First occurrence should be kept
        let findings = vec![
            make_finding(Some("a.rs"), Some(1), "first"),
            make_finding(Some("b.rs"), Some(2), "second"),
            make_finding(Some("a.rs"), Some(1), "duplicate-first"),
        ];
        let deduped = deduplicate_findings(findings);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].message, "first");
        assert_eq!(deduped[1].message, "second");
    }

    #[test]
    fn test_post_process_findings_empty() {
        let result = post_process_findings(&[]);
        assert!(result.is_empty());
    }
}
