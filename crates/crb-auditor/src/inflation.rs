use crb_shared::pattern::{has_pattern, make_pattern_list, PatternProvider};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct InflatedCategory {
    pub name: &'static str,

    pub patterns: &'static [&'static str],

    pub description: &'static str,

    pub downgrade_quantum: i32,
}

impl PatternProvider for InflatedCategory {
    fn name(&self) -> &'static str {
        self.name
    }

    fn patterns(&self) -> &[&'static str] {
        self.patterns
    }
}

static INFLATED_CATEGORIES: LazyLock<Vec<InflatedCategory>> = LazyLock::new(|| {
    vec![
        InflatedCategory {
            name: "architecture_nits",
            patterns: &[
                // SOLID principle violations
                r"\b(SRP|DIP|OCP)( violation)?\b",
                r"\b(Single Responsibility|Dependency Inversion|Open/Closed) Principle\b",
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
                r"\b(could|should) be (extracted|refactored|moved|separated)\b",
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

static INFLATED_RE: LazyLock<Vec<(&'static str, &'static str, Regex)>> =
    LazyLock::new(|| make_pattern_list(INFLATED_CATEGORIES.as_ref()));

/// Match a finding against INFLATED_PATTERNS.
///
/// Returns `(category_name, matching_pattern)` if a match is found.
pub fn has_inflated_pattern(
    finding_text: &str,
    evidence: &str,
) -> Option<(&'static str, &'static str)> {
    has_pattern(finding_text, evidence, &INFLATED_RE)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_inflated_pattern() {
        let result = has_inflated_pattern("SRP violation in UserService", "Class does too much");
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "architecture_nits");

        let result = has_inflated_pattern(
            "Could cause a performance issue",
            "If not careful, this could lead to slowness",
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "hypothetical_theoretical");

        let result = has_inflated_pattern(
            "Naming convention is inconsistent",
            "camelCase vs snake_case",
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "style_nits");

        let result = has_inflated_pattern("This is a real bug", "Memory corruption detected");
        assert!(result.is_none());
    }
}
