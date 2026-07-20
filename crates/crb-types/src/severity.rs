use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, IntoStaticStr};

/// Severity levels for findings, ordered from most to least severe.
///
/// We support an array of aliases for each level so to give the LLM output
/// a better chance of matching the expected severity level.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(sea_orm::EnumIter, sea_orm::DeriveActiveEnum),
    sea_orm(rs_type = "String", db_type = "Text")
)]
#[cfg_attr(not(feature = "seaorm-storage"), derive(strum::EnumIter))]
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
    Display,
    IntoStaticStr,
    JsonSchema,
    Hash,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Security vulnerabilities or correctness bugs.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Critical"))]
    #[serde(alias = "crit", alias = "error")]
    Critical = 0,

    /// Significant issues that should be addressed soon.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "High"))]
    High = 1,

    /// Moderate issues that should be reviewed.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Medium"))]
    #[default]
    #[serde(alias = "med")]
    Medium = 2,

    /// Minor issues or style concerns.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Low"))]
    #[serde(alias = "minor")]
    Low = 3,

    /// Observations without actionable impact.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Info"))]
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
