//! Port of `severity_auditor.py` - rule-based severity downgrade for inflated findings.
//!
//! Detects and downgrades inflated severity labels in code-review findings.
//! Only downgrades never upgrades. Protects security, data-integrity,
//! and correctness bugs with NEVER_DOWNGRADE_PATTERNS guard.
//!
//! Adds audit trail fields: `severity_audited` and `severity_audit_reason`.

pub mod deflation;
pub mod inflation;

use crb_shared::{finding::Finding, severity::Severity};

use crate::{
    deflation::has_never_downgrade_pattern,
    inflation::{downgrade_quantum, has_inflated_pattern},
};

/// Apply severity auditing to a list of findings.
///
/// For each finding:
/// 1. Check NEVER_DOWNGRADE patterns first (if matched, skip downgrade)
/// 2. Check multi-agent CRITICAL findings (skip if ≥2 agents)
/// 3. Check against INFLATED_PATTERNS; if matched, apply downgrade
/// 4. Only downgrade - never upgrade
/// 5. Add `severity_audited` and `severity_audit_reason` fields
pub fn apply_severity_auditor(findings: Vec<Finding>) -> Vec<Finding> {
    let mut modified_findings = Vec::new();

    for mut finding in findings {
        let finding_text = &finding.message;
        let evidence = finding.evidence.clone().unwrap_or_default();
        let current_severity = Severity::from_str(&finding.severity);
        let num_agents = finding.agent_count.unwrap_or(0);

        finding.severity = current_severity.as_str();

        if let Some((protection_category, _)) = has_never_downgrade_pattern(finding_text, &evidence)
        {
            finding.severity_audited = false;
            finding.severity_audit_reason = Some(format!(
                "protected_by_never_downgrade_pattern: {}",
                protection_category
            ));
            modified_findings.push(finding);
            continue;
        }

        if current_severity == Severity::Critical && num_agents >= 2 {
            finding.severity_audited = false;
            finding.severity_audit_reason = Some(format!(
                "protected_by_multi_agent_critical: {}_agents",
                num_agents
            ));
            modified_findings.push(finding);
            continue;
        }

        if let Some((match_category, match_pattern)) = has_inflated_pattern(finding_text, &evidence)
        {
            if let Some(quantum) = downgrade_quantum(match_category) {
                let new_severity = current_severity.apply_quantum(quantum);

                if new_severity >= current_severity {
                    finding.severity = new_severity.as_str();
                    finding.severity_audited = true;
                    finding.severity_audit_reason = Some(format!(
                        "downgraded: {}->{} by category='{}' pattern='{}' (quantum={})",
                        current_severity.as_str(),
                        new_severity.as_str(),
                        match_category,
                        match_pattern,
                        quantum
                    ));
                } else {
                    finding.severity_audited = true;
                    finding.severity_audit_reason = Some(format!(
                        "matched_category={} but no_downgrade_needed",
                        match_category
                    ));
                }
            } else {
                finding.severity_audited = true;
                finding.severity_audit_reason = Some("no_inflated_patterns_matched".to_string());
            }
        } else {
            finding.severity_audited = true;
            finding.severity_audit_reason = Some("no_inflated_patterns_matched".to_string());
        }

        modified_findings.push(finding);
    }

    modified_findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_finding(text: &str, severity: &str, evidence: &str, num_agents: u64) -> Finding {
        Finding {
            message: text.to_string(),
            severity: severity.to_string(),
            evidence: if evidence.is_empty() {
                None
            } else {
                Some(evidence.to_string())
            },
            agent_count: Some(num_agents),
            ..Default::default()
        }
    }

    fn assert_critical_protected_from_downgrade(result: &[Finding]) {
        assert_eq!(result[0].severity, "critical");
        let reason = result[0].severity_audit_reason.as_deref().unwrap_or("");
        assert!(reason.contains("protected_by_never_downgrade"));
    }

    #[test]
    fn test_apply_severity_auditor_srp() {
        let f = make_finding(
            "SRP violation in UserService class — should be refactored",
            "high",
            "UserService handles both auth and profile",
            1,
        );
        let result = apply_severity_auditor(vec![f]);
        assert_eq!(result[0].severity, "low");
        assert_eq!(result[0].severity_audited, true);
    }

    #[test]
    fn test_apply_severity_auditor_sql_injection() {
        let f = make_finding(
            "SQL injection vulnerability in login query",
            "critical",
            "Raw string concatenation with user input",
            1,
        );
        let result = apply_severity_auditor(vec![f]);
        assert_critical_protected_from_downgrade(&result);
    }

    #[test]
    fn test_apply_severity_auditor_naming() {
        let f = make_finding(
            "The naming convention is inconsistent",
            "high",
            "camelCase vs snake_case",
            1,
        );
        let result = apply_severity_auditor(vec![f]);
        assert_eq!(result[0].severity, "low");
        assert_eq!(result[0].severity_audited, true);
    }

    #[test]
    fn test_apply_severity_auditor_hypothetical() {
        let f = make_finding(
            "Could cause a performance issue in production",
            "high",
            "If not careful, this could lead to slowness",
            2,
        );
        let result = apply_severity_auditor(vec![f]);
        assert_eq!(result[0].severity, "medium");
        assert_eq!(result[0].severity_audited, true);
    }

    #[test]
    fn test_apply_severity_auditor_null_pointer() {
        let f = make_finding(
            "Null pointer exception possible in line 42",
            "high",
            "obj.method() without null check",
            1,
        );
        let result = apply_severity_auditor(vec![f]);
        assert_eq!(result[0].severity, "high");
        assert_eq!(result[0].severity_audited, false);
    }

    #[test]
    fn test_apply_severity_auditor_multi_agent_critical() {
        let f = make_finding(
            "Architecture abstraction leak",
            "critical",
            "Service layer directly accesses DB",
            3,
        );
        let result = apply_severity_auditor(vec![f]);
        assert_eq!(result[0].severity, "critical");
        let reason = result[0].severity_audit_reason.as_deref().unwrap_or("");
        assert!(reason.contains("protected_by_multi_agent_critical"));
    }

    #[test]
    fn test_apply_severity_auditor_race_condition() {
        let f = make_finding(
            "Race condition in cache update logic",
            "critical",
            "Two concurrent writes without lock",
            1,
        );
        let result = apply_severity_auditor(vec![f]);
        assert_critical_protected_from_downgrade(&result);
    }

    #[test]
    fn test_downgrade_quantum() {
        assert_eq!(downgrade_quantum("architecture_nits"), Some(-2));
        assert_eq!(downgrade_quantum("hypothetical_theoretical"), Some(-1));
        assert_eq!(downgrade_quantum("style_nits"), Some(-3));
        assert_eq!(downgrade_quantum("nonexistent"), None);
    }

    #[test]
    fn test_empty_evidence() {
        let f = make_finding("SRP violation", "high", "", 1);
        let result = apply_severity_auditor(vec![f]);
        assert_eq!(result[0].severity, "low");
    }

    #[test]
    fn test_no_match_no_change() {
        let f = make_finding(
            "This is a genuine comment about code",
            "medium",
            "Just a comment",
            1,
        );
        let result = apply_severity_auditor(vec![f]);
        assert_eq!(result[0].severity, "medium");
        let reason = result[0].severity_audit_reason.as_deref().unwrap_or("");
        assert_eq!(reason, "no_inflated_patterns_matched");
    }
}
