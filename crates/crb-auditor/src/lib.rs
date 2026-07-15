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
pub fn apply_severity_auditor(findings: &mut Vec<Finding>) {
    findings.iter_mut().for_each(|finding| {
        let evidence = finding.evidence.clone().unwrap_or_default();
        let num_agents = finding.agent_count.unwrap_or(0);

        if let Some((protection_category, _)) =
            has_never_downgrade_pattern(&finding.message, &evidence)
        {
            finding.severity_audited = false;
            finding.severity_audit_reason = Some(format!(
                "protected_by_never_downgrade_pattern: {}",
                protection_category
            ));
            return;
        }

        if finding.severity == Severity::Critical && num_agents >= 2 {
            finding.severity_audited = false;
            finding.severity_audit_reason = Some(format!(
                "protected_by_multi_agent_critical: {}_agents",
                num_agents
            ));
            return;
        }

        let Some((match_category, match_pattern)) =
            has_inflated_pattern(&finding.message, &evidence)
        else {
            finding.severity_audited = true;
            finding.severity_audit_reason = Some("no_inflated_patterns_matched".to_string());
            return;
        };

        let Some(quantum) = downgrade_quantum(match_category) else {
            finding.severity_audited = true;
            finding.severity_audit_reason = Some("no_inflated_patterns_matched".to_string());
            return;
        };

        let new_severity = finding.severity.apply_quantum(quantum);

        if new_severity < finding.severity {
            finding.severity_audited = true;
            finding.severity_audit_reason = Some(format!(
                "matched_category={} but no_downgrade_needed",
                match_category
            ));
            return;
        }

        finding.severity = new_severity;
        finding.severity_audited = true;
        finding.severity_audit_reason = Some(format!(
            "downgraded: {}->{} by category='{}' pattern='{}' (quantum={})",
            finding.severity.as_str(),
            new_severity.as_str(),
            match_category,
            match_pattern,
            quantum
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_finding(text: &str, severity: Severity, evidence: &str, num_agents: u64) -> Finding {
        Finding {
            message: text.to_string(),
            severity: severity,
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
        assert_eq!(result[0].severity, Severity::Critical);
        let reason = result[0].severity_audit_reason.as_deref().unwrap_or("");
        assert!(reason.contains("protected_by_never_downgrade"));
    }

    #[test]
    fn test_apply_severity_auditor_srp() {
        let mut findings = vec![make_finding(
            "SRP violation in UserService class — should be refactored",
            Severity::High,
            "UserService handles both auth and profile",
            1,
        )];
        apply_severity_auditor(&mut findings);
        assert_eq!(findings[0].severity, Severity::Low);
        assert_eq!(findings[0].severity_audited, true);
    }

    #[test]
    fn test_apply_severity_auditor_sql_injection() {
        let mut findings = vec![make_finding(
            "SQL injection vulnerability in login query",
            Severity::Critical,
            "Raw string concatenation with user input",
            1,
        )];
        apply_severity_auditor(&mut findings);
        assert_critical_protected_from_downgrade(&findings);
    }

    #[test]
    fn test_apply_severity_auditor_naming() {
        let mut findings = vec![make_finding(
            "The naming convention is inconsistent",
            Severity::High,
            "camelCase vs snake_case",
            1,
        )];
        apply_severity_auditor(&mut findings);
        // Naming matches style_nits pattern with quantum -3: High→Info
        assert_eq!(findings[0].severity, Severity::Info);
        assert_eq!(findings[0].severity_audited, true);
    }

    #[test]
    fn test_apply_severity_auditor_hypothetical() {
        let mut findings = vec![make_finding(
            "Could cause a performance issue in production",
            Severity::High,
            "If not careful, this could lead to slowness",
            2,
        )];
        apply_severity_auditor(&mut findings);
        assert_eq!(findings[0].severity, Severity::Medium);
        assert_eq!(findings[0].severity_audited, true);
    }

    #[test]
    fn test_apply_severity_auditor_null_pointer() {
        let mut findings = vec![make_finding(
            "Null pointer exception possible in line 42",
            Severity::High,
            "obj.method() without null check",
            1,
        )];
        apply_severity_auditor(&mut findings);
        assert_eq!(findings[0].severity, Severity::High);
        assert_eq!(findings[0].severity_audited, false);
    }

    #[test]
    fn test_apply_severity_auditor_multi_agent_critical() {
        let mut findings = vec![make_finding(
            "Architecture abstraction leak",
            Severity::Critical,
            "Service layer directly accesses DB",
            3,
        )];
        apply_severity_auditor(&mut findings);
        assert_eq!(findings[0].severity, Severity::Critical);
        let reason = findings[0].severity_audit_reason.as_deref().unwrap_or("");
        assert!(reason.contains("protected_by_multi_agent_critical"));
    }

    #[test]
    fn test_apply_severity_auditor_race_condition() {
        let mut findings = vec![make_finding(
            "Race condition in cache update logic",
            Severity::Critical,
            "Two concurrent writes without lock",
            1,
        )];
        apply_severity_auditor(&mut findings);
        assert_critical_protected_from_downgrade(&findings);
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
        let mut findings = vec![make_finding("SRP violation", Severity::High, "", 1)];
        apply_severity_auditor(&mut findings);
        assert_eq!(findings[0].severity, Severity::Low);
    }

    #[test]
    fn test_no_match_no_change() {
        let mut findings = vec![make_finding(
            "This is a genuine comment about code",
            Severity::Medium,
            "Just a comment",
            1,
        )];
        apply_severity_auditor(&mut findings);
        assert_eq!(findings[0].severity, Severity::Medium);
        let reason = findings[0].severity_audit_reason.as_deref().unwrap_or("");
        assert_eq!(reason, "no_inflated_patterns_matched");
    }
}
