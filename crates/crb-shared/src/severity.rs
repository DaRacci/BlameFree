use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

/// Severity levels for findings, ordered from most to least severe.
///
/// # Ord
///
/// `Critical < High < Medium < Low < Info`
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, EnumIter, Display,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Critical severity — security vulnerabilities or correctness bugs.
    Critical = 0,
    /// High severity — significant issues that should be addressed soon.
    High = 1,
    /// Medium severity — moderate issues that should be reviewed.
    Medium = 2,
    /// Low severity — minor issues or style concerns.
    Low = 3,
    /// Informational — observations without actionable impact.
    Info = 4,
}

impl Severity {
    /// Parse a severity from a string, falling back to Medium on unrecognized input.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        serde_json::from_str(&format!("\"{}\"", s)).unwrap_or(Self::Medium)
    }

    /// Canonical lowercase string representation.
    #[allow(clippy::unwrap_used)]
    pub fn as_str(&self) -> String {
        serde_json::to_string(self)
            .unwrap()
            .trim_matches('"')
            .to_string()
    }

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

/// Compute the new severity label after applying a downgrade quantum.
///
/// `quantum` is negative (e.g., -2 means reduce severity by 2 levels).
pub fn compute_new_severity(current: &str, quantum: i32) -> String {
    Severity::from_str(current).apply_quantum(quantum).as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_new_severity() {
        assert_eq!(compute_new_severity("high", -2), "low");
        assert_eq!(compute_new_severity("medium", -2), "info");
        assert_eq!(compute_new_severity("critical", -2), "medium");
        assert_eq!(compute_new_severity("high", -1), "medium");
        assert_eq!(compute_new_severity("low", -1), "info");
        assert_eq!(compute_new_severity("high", -3), "info");
        assert_eq!(compute_new_severity("info", -1), "info");
        assert_eq!(compute_new_severity("info", 0), "info");
        assert_eq!(compute_new_severity("low", -2), "info");
    }
}
