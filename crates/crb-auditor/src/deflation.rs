use crb_shared::pattern::{PatternProvider, has_pattern, make_pattern_list};
use regex::Regex;
use std::sync::LazyLock;

/// A protected category that should never be downgraded.
#[derive(Debug)]
pub struct ProtectionCategory {
    /// Canonical name for this protection category.
    pub name: &'static str,

    /// Regex patterns that trigger this protection.
    pub patterns: &'static [&'static str],
}

impl PatternProvider for ProtectionCategory {
    fn name(&self) -> &'static str {
        self.name
    }

    fn patterns(&self) -> &'static [&'static str] {
        self.patterns
    }
}

static NEVER_DOWNGRADE_CATEGORIES: LazyLock<Vec<ProtectionCategory>> = LazyLock::new(|| {
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

static NEVER_DOWNGRADE_RE: LazyLock<Vec<(&'static str, &'static str, Regex)>> =
    LazyLock::new(|| make_pattern_list(NEVER_DOWNGRADE_CATEGORIES.as_ref()));

/// Check if a finding matches any NEVER_DOWNGRADE pattern.
///
/// Returns `(category_name, matching_pattern)` if a match is found.
pub fn has_never_downgrade_pattern(
    finding_text: &str,
    evidence: &str,
) -> Option<(&'static str, &'static str)> {
    has_pattern(finding_text, evidence, &NEVER_DOWNGRADE_RE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_never_downgrade_pattern() {
        let result = has_never_downgrade_pattern(
            "SQL injection vulnerability in login",
            "Raw string concatenation",
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "security_vulns");

        let result =
            has_never_downgrade_pattern("SRP violation in UserService", "Class does too much");
        assert!(result.is_none());

        let result =
            has_never_downgrade_pattern("Race condition in cache update", "Two concurrent writes");
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "data_integrity");

        let result = has_never_downgrade_pattern(
            "Null pointer exception possible",
            "obj.method() without null check",
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "correctness_bugs");
    }
}
