use serde::Deserialize;
use serde::Serialize;

/// Aggregated evaluation metrics computed from judge verdicts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[deprecated = "Use [`crb-types::benchmark::Metrics`] instead"]
pub struct Metrics {
    /// Number of true positives.
    pub true_positives: usize,

    /// Number of false positives.
    pub false_positives: usize,

    /// Number of false negatives.
    pub false_negatives: usize,

    /// Precision = tp / (tp + fp).
    pub precision: f64,

    /// Recall = tp / (tp + fn).
    pub recall: f64,

    /// The harmonic mean of precision and recall.
    pub f1: f64,
}
