use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

/// Severity levels for findings, ordered from most to least severe.
///
/// We support an array of aliases for each level so to give the LLM output
/// a better chance of matching the expected severity level.
#[derive(
    Default,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    EnumIter,
    Display,
    JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Security vulnerabilities or correctness bugs.
    #[serde(alias = "crit", alias = "error")]
    Critical = 0,

    /// Significant issues that should be addressed soon.
    High = 1,

    /// Moderate issues that should be reviewed.
    #[default]
    #[serde(alias = "med")]
    Medium = 2,

    /// Minor issues or style concerns.
    #[serde(alias = "minor")]
    Low = 3,

    /// Observations without actionable impact.
    #[serde(alias = "information", alias = "informational", alias = "trivial")]
    Info = 4,
}

impl Severity {
    /// Shift severity by `quantum` (negative = downgrade), clamped to valid range.
    pub fn apply_quantum(&self, quantum: i32) -> Self {
        let new_val = ((*self as i32) - quantum).clamp(0, 4) as u8;
        match new_val {
            0 => Severity::Critical,
            1 => Severity::High,
            2 => Severity::Medium,
            3 => Severity::Low,
            _ => Severity::Info,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_new_severity() {
        assert_eq!(Severity::High.apply_quantum(-2), Severity::Low);
        assert_eq!(Severity::Medium.apply_quantum(-2), Severity::Info);
        assert_eq!(Severity::Critical.apply_quantum(-2), Severity::Medium);
        assert_eq!(Severity::High.apply_quantum(-1), Severity::Medium);
        assert_eq!(Severity::Low.apply_quantum(-1), Severity::Info);
        assert_eq!(Severity::High.apply_quantum(-3), Severity::Info);
        assert_eq!(Severity::Info.apply_quantum(-1), Severity::Info);
        assert_eq!(Severity::Info.apply_quantum(0), Severity::Info);
        assert_eq!(Severity::Low.apply_quantum(-2), Severity::Info);
    }
}
