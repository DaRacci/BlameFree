//! Port of `severity_auditor.py` — rule-based severity downgrade for inflated findings.
//!
//! Detects and downgrades inflated severity labels in code-review findings.
//! Only downgrades — never upgrades. Protects genuine security, data-integrity,
//! and correctness bugs via NEVER_DOWNGRADE_PATTERNS guard.
//!
//! Adds audit trail fields: `severity_audited` and `severity_audit_reason`.

pub use crb_agents::Finding;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;

// =============================================================================
// SEVERITY ORDER
// =============================================================================

/// Numeric ordering for comparison. Lower number = more severe.
pub const SEVERITY_ORDER: &[(&str, u8)] = &[
    ("critical", 0),
    ("high", 1),
    ("medium", 2),
    ("low", 3),
];

/// Convert a severity label to its numeric value.
pub fn severity_value(severity: &str) -> u8 {
    let normalized = normalize_severity(severity);
    for (name, val) in SEVERITY_ORDER {
        if *name == normalized {
            return *val;
        }
    }
    2 // default medium
}

fn normalize_severity(severity: &str) -> String {
    if severity.is_empty() {
        return "medium".to_string();
    }
    severity.trim().to_lowercase()
}

// =============================================================================
// INFLATED CATEGORIES
// =============================================================================

/// A category of inflated severity findings.
#[derive(Debug)]
pub struct InflatedCategory {
    pub name: &'static str,
    pub patterns: &'static [&'static str],
    pub description: &'static str,
    pub downgrade_quantum: i32,
}

/// A protected category that should never be downgraded.
#[derive(Debug)]
pub struct ProtectionCategory {
    pub name: &'static str,
    pub patterns: &'static [&'static str],
}

fn make_inflated_re_list() -> Vec<(&'static str, &'static str, Regex)> {
    let mut result = Vec::new();
    for cat in INFLATED_CATEGORIES.iter() {
        for &pat in cat.patterns.iter() {
            if let Ok(re) = Regex::new(&format!("(?i){}", pat)) {
                result.push((cat.name, pat, re));
            }
        }
    }
    result
}

/// All inflated categories, compiled once.
pub static INFLATED_CATEGORIES: Lazy<Vec<InflatedCategory>> = Lazy::new(|| {
    vec![
        InflatedCategory {
            name: "architecture_nits",
            patterns: &[
                // SOLID principle violations
                r"\bSRP( violation)?\b",
                r"\bSingle Responsibility Principle\b",
                r"\bDIP( violation)?\b",
                r"\bDependency Inversion( Principle)?\b",
                r"\bOCP( violation)?\b",
                r"\bOpen/Closed( Principle)?\b",
                // Anti-pattern / code smell names
                r"\bGod class\b",
                r"\bfeature envy\b",
                r"\b(Inappropriate Intimacy)\b",
                r"\b(Low |Lack of )?Cohesion\b",
                r"\b(high|tight|coupling) (coupling|dependency)\b",
                // Design pattern commentary
                r"\bdesign pattern (violation|not followed|misuse)\b",
                r"\banti.?pattern\b",
                // Refactoring suggestions
                r"\bshould be refactored\b",
                r"\bshould be extracted\b",
                r"\bcould be (extracted|refactored|moved|separated)\b",
                // Abstraction / architecture observations
                r"\babstract(ion)? leak(age)?\b",
                r"\bleaky abstraction\b",
            ],
            description: "Architecture/style observations framed as HIGH/CRITICAL bugs",
            downgrade_quantum: -2,
        },
        InflatedCategory {
            name: "hypothetical_theoretical",
            patterns: &[
                // Speculative language
                r"\bcould cause\b",
                r"\bmight lead to\b",
                r"\bmay result in\b",
                r"\b(potential|possibly) (issue|bug|problem|vulnerability|risk)\b",
                r"\bfor (future|scalability|maintainability)\b",
                r"\bin (the )?future\b",
                r"\bin theory\b",
                r"\btheoretically\b",
                r"\bif not careful\b",
                r"\bwhat if\b",
                r"\bsuppose\b",
                // Hedge language
                r"\bin some cases\b",
                r"\bmight (be|have|cause|lead)\b",
                r"\bcould (potentially|possibly)\b",
            ],
            description: "Hypothetical/theoretical concerns with no concrete exploit path",
            downgrade_quantum: -1,
        },
        InflatedCategory {
            name: "style_nits",
            patterns: &[
                // Naming and formatting
                r"\bnaming (convention|style|choice)\b",
                r"\bformatting\b",
                r"\bwhitespace\b",
                r"\bindentation\b",
                r"\bcosmetic\b",
                // Cleanup suggestions
                r"\bcould be simplified\b",
                r"\bcould be cleaned up\b",
                r"\bcould use better\b",
                r"\bminor (nit|style|issue)\b",
                // Magic numbers/strings
                r"\bmag(n)?ic (number|string|value)\b",
                r"\bhardcoded (value|string|number)\b",
            ],
            description: "Style/cosmetic preferences masquerading as bugs",
            downgrade_quantum: -3,
        },
    ]
});

/// Compile all inflated patterns into regexes for fast matching.
pub static INFLATED_RE: Lazy<Vec<(&'static str, &'static str, Regex)>> = Lazy::new(make_inflated_re_list);

// =============================================================================
// NEVER DOWNGRADE PATTERNS
// =============================================================================

/// All protection categories, compiled once.
pub static NEVER_DOWNGRADE_CATEGORIES: Lazy<Vec<ProtectionCategory>> = Lazy::new(|| {
    vec![
        ProtectionCategory {
            name: "security_vulns",
            patterns: &[
                r"\bSQL injection\b",
                r"\bXSS\b",
                r"\bcross.?site (scripting|request forgery)\b",
                r"\bCSRF\b",
                r"\bauth.? bypass\b",
                r"\bauthentication bypass\b",
                r"\bprivilege escalation\b",
                r"\bdata (exposure|leak|breach|exfiltration)\b",
                r"\bremote code execution\b",
                r"\bRCE\b",
                r"\bcommand injection\b",
                r"\bpath traversal\b",
                r"\bSSRF\b",
                r"\bServer Side Request Forgery\b",
                r"\bXXE\b",
                r"\bXML External Entity\b",
                r"\bdeserialization\b",
                r"\binsecure direct object reference\b",
                r"\bIDOR\b",
                r"\bsensitive data exposure\b",
            ],
        },
        ProtectionCategory {
            name: "data_integrity",
            patterns: &[
                r"\bdata loss\b",
                r"\bdata corruption\b",
                r"\bdeadlock\b",
                r"\blivelock\b",
                r"\brace condition\b",
                r"\btransaction (lost|unsafe|incomplete|inconsistency)\b",
                r"\bdatabase (corruption|inconsistency)\b",
            ],
        },
        ProtectionCategory {
            name: "correctness_bugs",
            patterns: &[
                r"\bwrong (value|result|calculation|output)\b",
                r"\bincorrect (logic|condition|bound|calculation)\b",
                r"\bcrash(es|ing)?\b",
                r"\bnull pointer\b",
                r"\bNPE\b",
                r"\bsegfault\b",
                r"\bmemory corruption\b",
                r"\bmemory leak\b",
                r"\bnull reference\b",
                r"\bindex out of bounds\b",
                r"\btype error\b",
                r"\bkey error\b",
                r"\battribute error\b",
            ],
        },
    ]
});

/// Compile all never-downgrade patterns into regexes.
pub static NEVER_DOWNGRADE_RE: Lazy<Vec<(&'static str, Regex)>> = Lazy::new(|| {
    let mut result = Vec::new();
    for cat in NEVER_DOWNGRADE_CATEGORIES.iter() {
        for &pat in cat.patterns.iter() {
            if let Ok(re) = Regex::new(&format!("(?i){}", pat)) {
                result.push((cat.name, re));
            }
        }
    }
    result
});

// =============================================================================
// HELPERS
// =============================================================================

/// Compute the new severity label after applying a downgrade quantum.
///
/// `quantum` is negative (e.g., -2 means reduce severity by 2 levels).
pub fn compute_new_severity(current: &str, quantum: i32) -> String {
    let current_val = severity_value(current) as i32;
    // quantum is negative: current_val - (-2) = current_val + 2 (less severe)
    let new_val = (current_val - quantum).min(3).max(0) as u8;
    for (name, val) in SEVERITY_ORDER {
        if *val == new_val {
            return name.to_string();
        }
    }
    "low".to_string()
}

/// Check if a finding matches any NEVER_DOWNGRADE pattern.
///
/// Returns the category name if a match is found, or `None`.
pub fn has_never_downgrade_pattern(finding_text: &str, evidence: &str) -> Option<&'static str> {
    let combined = format!("{} {}", finding_text, evidence);

    for (category, re) in NEVER_DOWNGRADE_RE.iter() {
        if re.is_match(&combined) {
            return Some(category);
        }
    }
    None
}

/// Match a finding against INFLATED_PATTERNS.
///
/// Returns `(category_name, matching_pattern)` if a match is found.
pub fn match_inflated_pattern(
    finding_text: &str,
    evidence: &str,
) -> Option<(&'static str, &'static str)> {
    let combined = format!("{} {}", finding_text, evidence);

    for (category, pattern, re) in INFLATED_RE.iter() {
        if re.is_match(&combined) {
            return Some((category, pattern));
        }
    }
    None
}

/// Return the downgrade quantum for a given inflated category name.
pub fn downgrade_quantum(category: &str) -> Option<i32> {
    for cat in INFLATED_CATEGORIES.iter() {
        if cat.name == category {
            return Some(cat.downgrade_quantum);
        }
    }
    None
}

// =============================================================================
// CORE API
// =============================================================================

/// Apply severity auditing to a list of findings.
///
/// For each finding:
/// 1. Check NEVER_DOWNGRADE patterns first (if matched, skip downgrade)
/// 2. Check multi-agent CRITICAL findings (skip if ≥2 agents)
/// 3. Check against INFLATED_PATTERNS; if matched, apply downgrade
/// 4. Only downgrade — never upgrade
/// 5. Add `severity_audited` and `severity_audit_reason` fields
pub fn apply_severity_auditor(findings: Vec<Map<String, Value>>) -> Vec<Map<String, Value>> {
    let mut modified_findings = Vec::new();

    for finding in findings {
        let finding_text = finding
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let evidence = finding
            .get("evidence")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let current_severity = normalize_severity(
            finding
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("medium"),
        );
        let num_agents = finding
            .get("num_agents")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            + finding
                .get("agent_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

        let mut modified = finding.clone();
        modified.insert(
            "severity".to_string(),
            Value::String(current_severity.clone()),
        );

        // === STEP 1: Check NEVER_DOWNGRADE patterns ===
        if let Some(protection_category) = has_never_downgrade_pattern(&finding_text, &evidence) {
            modified.insert("severity_audited".to_string(), Value::Bool(false));
            modified.insert(
                "severity_audit_reason".to_string(),
                Value::String(format!(
                    "protected_by_never_downgrade_pattern: {}",
                    protection_category
                )),
            );
            modified_findings.push(modified);
            continue;
        }

        // === STEP 2: Check multi-agent CRITICAL findings ===
        if current_severity == "critical" && num_agents >= 2 {
            modified.insert("severity_audited".to_string(), Value::Bool(false));
            modified.insert(
                "severity_audit_reason".to_string(),
                Value::String(format!(
                    "protected_by_multi_agent_critical: {}_agents",
                    num_agents
                )),
            );
            modified_findings.push(modified);
            continue;
        }

        // === STEP 3: Check against INFLATED_PATTERNS ===
        if let Some((match_category, match_pattern)) =
            match_inflated_pattern(&finding_text, &evidence)
        {
            // === STEP 4: Apply downgrade ===
            if let Some(quantum) = downgrade_quantum(match_category) {
                let new_severity = compute_new_severity(&current_severity, quantum);

                // Only downgrade — never upgrade
                if severity_value(&new_severity) >= severity_value(&current_severity) {
                    modified.insert(
                        "severity".to_string(),
                        Value::String(new_severity.clone()),
                    );
                    modified.insert("severity_audited".to_string(), Value::Bool(true));
                    modified.insert(
                        "severity_audit_reason".to_string(),
                        Value::String(format!(
                            "downgraded: {}→{} by category='{}' pattern='{}' (quantum={})",
                            current_severity, new_severity, match_category, match_pattern, quantum
                        )),
                    );
                } else {
                    modified.insert("severity_audited".to_string(), Value::Bool(true));
                    modified.insert(
                        "severity_audit_reason".to_string(),
                        Value::String(format!(
                            "matched_category={} but no_downgrade_needed",
                            match_category
                        )),
                    );
                }
            } else {
                modified.insert("severity_audited".to_string(), Value::Bool(true));
                modified.insert(
                    "severity_audit_reason".to_string(),
                    Value::String("no_inflated_patterns_matched".to_string()),
                );
            }
        } else {
            modified.insert("severity_audited".to_string(), Value::Bool(true));
            modified.insert(
                "severity_audit_reason".to_string(),
                Value::String("no_inflated_patterns_matched".to_string()),
            );
        }

        modified_findings.push(modified);
    }

    modified_findings
}

// =============================================================================
// REPORTING
// =============================================================================

/// Generate a human-readable report comparing findings before and after auditing.
pub fn format_severity_audit_report(
    before: &[Map<String, Value>],
    after: &[Map<String, Value>],
) -> String {
    let total_before = before.len();
    let total_after = after.len();

    let mut downgraded: Vec<(&Map<String, Value>, &Map<String, Value>, String, String)> =
        Vec::new();
    let mut protected: Vec<(&Map<String, Value>, &Map<String, Value>)> = Vec::new();
    let mut skipped: Vec<(&Map<String, Value>, &Map<String, Value>)> = Vec::new();

    for (fb, fa) in before.iter().zip(after.iter()) {
        let original_sev = normalize_severity(
            fb.get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("medium"),
        );
        let new_sev = normalize_severity(
            fa.get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("medium"),
        );
        let reason = fa
            .get("severity_audit_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if reason.contains("protected_by_never_downgrade") {
            protected.push((fb, fa));
        } else if reason.contains("protected_by_multi_agent_critical") {
            skipped.push((fb, fa));
        } else if reason.contains("downgraded:") {
            downgraded.push((fb, fa, original_sev, new_sev));
        }
    }

    // Count by category among downgraded
    let mut category_counts: HashMap<String, usize> = HashMap::new();
    for (_, fa, _, _) in &downgraded {
        let reason = fa
            .get("severity_audit_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(caps) = regex::Regex::new(r"category='([^']+)'")
            .ok()
            .and_then(|re| re.captures(reason))
        {
            let cat = caps[1].to_string();
            *category_counts.entry(cat).or_insert(0) += 1;
        }
    }

    let mut lines = vec![];
    lines.push("=".repeat(72));
    lines.push("SEVERITY AUDITOR REPORT".to_string());
    lines.push("=".repeat(72));
    lines.push(format!("Total findings checked:   {}", total_before));
    lines.push(format!("Total findings after:     {}", total_after));
    lines.push(String::new());

    lines.push(format!("Findings downgraded:      {}", downgraded.len()));
    for (cat, count) in &category_counts {
        let pct = (*count as f64 / downgraded.len().max(1) as f64) * 100.0;
        lines.push(format!("  - {}: {} ({:.1}%)", cat, count, pct));
    }
    lines.push(String::new());

    lines.push(format!("Protected (never-down):   {}", protected.len()));
    lines.push(format!("Skipped (multi-agent):    {}", skipped.len()));
    let preserved = total_before - downgraded.len();
    lines.push(format!(
        "Preserved at original:    {} ({:.1}%)",
        preserved,
        (preserved as f64 / total_before.max(1) as f64) * 100.0
    ));
    lines.push(String::new());

    // Show a few examples of downgraded findings
    if !downgraded.is_empty() {
        lines.push("Sample downgrades (first 5):".to_string());
        for (fb, fa, orig, new_) in downgraded.iter().take(5) {
            let text: String = fb
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(80)
                .collect();
            let reason = fa
                .get("severity_audit_reason")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let reason_short: String = reason.chars().take(100).collect();
            lines.push(format!("  [{}→{}] {}", orig, new_, text));
            lines.push(format!("    Reason: {}", reason_short));
        }
        lines.push(String::new());
    }

    lines.push("=".repeat(72));
    lines.join("\n")
}

// =============================================================================
// CONVENIENCE: BATCH PROCESSING
// =============================================================================

/// Convenience function: apply the severity auditor and generate a report.
pub fn audit_and_report(
    findings: Vec<Map<String, Value>>,
) -> (Vec<Map<String, Value>>, String) {
    let findings_before: Vec<Map<String, Value>> = findings.iter().cloned().collect();
    let findings_after = apply_severity_auditor(findings);
    let report = format_severity_audit_report(&findings_before, &findings_after);
    (findings_after, report)
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_finding(
        text: &str,
        severity: &str,
        evidence: &str,
        num_agents: u64,
    ) -> Map<String, Value> {
        let mut f = Map::new();
        f.insert("text".to_string(), Value::String(text.to_string()));
        f.insert("severity".to_string(), Value::String(severity.to_string()));
        f.insert("evidence".to_string(), Value::String(evidence.to_string()));
        f.insert("num_agents".to_string(), Value::Number(num_agents.into()));
        f
    }

    #[test]
    fn test_severity_value() {
        assert_eq!(severity_value("critical"), 0);
        assert_eq!(severity_value("high"), 1);
        assert_eq!(severity_value("medium"), 2);
        assert_eq!(severity_value("low"), 3);
        assert_eq!(severity_value("unknown"), 2);
    }

    #[test]
    fn test_has_never_downgrade_pattern() {
        // SQL injection should be protected
        let result = has_never_downgrade_pattern(
            "SQL injection vulnerability in login",
            "Raw string concatenation",
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "security_vulns");

        // SRP violation should not be protected
        let result =
            has_never_downgrade_pattern("SRP violation in UserService", "Class does too much");
        assert!(result.is_none());

        // Race condition should be protected (data_integrity)
        let result =
            has_never_downgrade_pattern("Race condition in cache update", "Two concurrent writes");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "data_integrity");

        // Null pointer should be protected (correctness_bugs)
        let result = has_never_downgrade_pattern(
            "Null pointer exception possible",
            "obj.method() without null check",
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "correctness_bugs");
    }

    #[test]
    fn test_match_inflated_pattern() {
        // SRP violation should match architecture_nits
        let result = match_inflated_pattern("SRP violation in UserService", "Class does too much");
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "architecture_nits");

        // Hypothetical language should match hypothetical_theoretical
        let result = match_inflated_pattern(
            "Could cause a performance issue",
            "If not careful, this could lead to slowness",
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "hypothetical_theoretical");

        // Naming convention should match style_nits
        let result = match_inflated_pattern(
            "Naming convention is inconsistent",
            "camelCase vs snake_case",
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "style_nits");

        // Random text should not match
        let result = match_inflated_pattern("This is a real bug", "Memory corruption detected");
        assert!(result.is_none());
    }

    #[test]
    fn test_compute_new_severity() {
        // HIGH with -2 quantum → LOW
        assert_eq!(compute_new_severity("high", -2), "low");
        // MEDIUM with -2 quantum → LOW
        assert_eq!(compute_new_severity("medium", -2), "low");
        // CRITICAL with -2 quantum → MEDIUM
        assert_eq!(compute_new_severity("critical", -2), "medium");
        // HIGH with -1 quantum → MEDIUM
        assert_eq!(compute_new_severity("high", -1), "medium");
        // LOW with -1 quantum → LOW (clamped)
        assert_eq!(compute_new_severity("low", -1), "low");
        // HIGH with -3 quantum → LOW
        assert_eq!(compute_new_severity("high", -3), "low");
    }

    #[test]
    fn test_apply_severity_auditor_srp() {
        // SRP violation → should be downgraded to low (architecture_nit, -2 quantum)
        let f = make_finding(
            "SRP violation in UserService class — should be refactored",
            "high",
            "UserService handles both auth and profile",
            1,
        );
        let findings = vec![f];
        let result = apply_severity_auditor(findings);
        assert_eq!(
            result[0]
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "low"
        );
        assert_eq!(
            result[0]
                .get("severity_audited")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            true
        );
    }

    #[test]
    fn test_apply_severity_auditor_sql_injection() {
        // SQL injection → should be protected (NEVER_DOWNGRADE)
        let f = make_finding(
            "SQL injection vulnerability in login query",
            "critical",
            "Raw string concatenation with user input",
            1,
        );
        let findings = vec![f];
        let result = apply_severity_auditor(findings);
        assert_eq!(
            result[0]
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "critical"
        );
        assert_eq!(
            result[0]
                .get("severity_audited")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            false
        );
        let reason = result[0]
            .get("severity_audit_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(reason.contains("protected_by_never_downgrade"));
    }

    #[test]
    fn test_apply_severity_auditor_naming() {
        // Naming convention → should be downgraded to low (style_nit, -3 quantum)
        let f = make_finding(
            "The naming convention is inconsistent",
            "high",
            "camelCase vs snake_case",
            1,
        );
        let findings = vec![f];
        let result = apply_severity_auditor(findings);
        assert_eq!(
            result[0]
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "low"
        );
        assert_eq!(
            result[0]
                .get("severity_audited")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            true
        );
    }

    #[test]
    fn test_apply_severity_auditor_hypothetical() {
        // "Could cause" → should be downgraded -1 level (HIGH→MEDIUM)
        let f = make_finding(
            "Could cause a performance issue in production",
            "high",
            "If not careful, this could lead to slowness",
            2,
        );
        let findings = vec![f];
        let result = apply_severity_auditor(findings);
        assert_eq!(
            result[0]
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "medium"
        );
        assert_eq!(
            result[0]
                .get("severity_audited")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            true
        );
    }

    #[test]
    fn test_apply_severity_auditor_null_pointer() {
        // Null pointer → should be protected (NEVER_DOWNGRADE)
        let f = make_finding(
            "Null pointer exception possible in line 42",
            "high",
            "obj.method() without null check",
            1,
        );
        let findings = vec![f];
        let result = apply_severity_auditor(findings);
        assert_eq!(
            result[0]
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "high"
        );
        assert_eq!(
            result[0]
                .get("severity_audited")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            false
        );
    }

    #[test]
    fn test_apply_severity_auditor_multi_agent_critical() {
        // Multi-agent CRITICAL (3 agents) → should be skipped
        let f = make_finding(
            "Architecture abstraction leak",
            "critical",
            "Service layer directly accesses DB",
            3,
        );
        let findings = vec![f];
        let result = apply_severity_auditor(findings);
        assert_eq!(
            result[0]
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "critical"
        );
        let reason = result[0]
            .get("severity_audit_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(reason.contains("protected_by_multi_agent_critical"));
    }

    #[test]
    fn test_apply_severity_auditor_race_condition() {
        // Race condition → should be protected (NEVER_DOWNGRADE, data_integrity)
        let f = make_finding(
            "Race condition in cache update logic",
            "critical",
            "Two concurrent writes without lock",
            1,
        );
        let findings = vec![f];
        let result = apply_severity_auditor(findings);
        assert_eq!(
            result[0]
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "critical"
        );
        let reason = result[0]
            .get("severity_audit_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(reason.contains("protected_by_never_downgrade"));
    }

    #[test]
    fn test_format_severity_audit_report() {
        let f1 = make_finding(
            "SRP violation — should be refactored",
            "high",
            "Handles too many concerns",
            1,
        );
        let f2 = make_finding(
            "SQL injection in query",
            "critical",
            "Raw string concatenation",
            1,
        );
        let before = vec![f1, f2];
        let after = apply_severity_auditor(before.clone());
        let report = format_severity_audit_report(&before, &after);
        assert!(report.contains("SEVERITY AUDITOR REPORT"));
        assert!(report.contains("Total findings checked:   2"));
        assert!(report.contains("Findings downgraded:"));
        assert!(report.contains("Protected (never-down):"));
    }

    #[test]
    fn test_audit_and_report() {
        let findings = vec![
            make_finding("SRP violation", "high", "Too many concerns", 1),
            make_finding(
                "Race condition",
                "critical",
                "Unsafe concurrent access",
                1,
            ),
        ];
        let (result, report) = audit_and_report(findings);
        assert_eq!(result.len(), 2);
        assert!(!report.is_empty());
    }

    #[test]
    fn test_downgrade_quantum() {
        assert_eq!(downgrade_quantum("architecture_nits"), Some(-2));
        assert_eq!(
            downgrade_quantum("hypothetical_theoretical"),
            Some(-1)
        );
        assert_eq!(downgrade_quantum("style_nits"), Some(-3));
        assert_eq!(downgrade_quantum("nonexistent"), None);
    }

    #[test]
    fn test_empty_evidence() {
        let f = make_finding("SRP violation", "high", "", 1);
        let result = apply_severity_auditor(vec![f]);
        assert_eq!(
            result[0]
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "low"
        );
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
        // No inflated pattern matches, no never-downgrade pattern matches
        // Should keep original severity
        assert_eq!(
            result[0]
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "medium"
        );
        let reason = result[0]
            .get("severity_audit_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(reason, "no_inflated_patterns_matched");
    }
}
