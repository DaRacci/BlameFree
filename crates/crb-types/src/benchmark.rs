use std::ops::AddAssign;

use serde::{Deserialize, Serialize};

/// Metrics data for a single PR evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsData {
    /// True positives count.
    pub true_positives: usize,

    /// False positives count.
    pub false_positives: usize,

    /// False negatives count.
    pub false_negatives: usize,

    /// Precision score.
    pub precision: f64,

    /// Recall score.
    pub recall: f64,

    /// F1 score.
    pub f1: f64,
}

/// Aggregate metrics across all PRs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metrics {
    /// Total true positives across all evaluated PRs.
    #[serde(alias = "total_tp")]
    pub true_positives: usize,

    /// Total false positives across all evaluated PRs.
    #[serde(alias = "total_fp")]
    pub false_positives: usize,

    /// Total false negatives across all evaluated PRs.
    #[serde(alias = "total_fn")]
    pub false_negatives: usize,

    /// Duration of the run in seconds.
    #[serde(default)]
    #[serde(alias = "elapsed")]
    pub duration_secs: f64,
}

impl Metrics {
    /// Aggregate precision across all evaluated items.
    pub fn precision(&self) -> f64 {
        if self.true_positives + self.false_positives > 0 {
            self.true_positives as f64 / (self.true_positives + self.false_positives) as f64
        } else {
            0.0
        }
    }

    /// Aggregate recall across all evaluated items.
    pub fn recall(&self) -> f64 {
        if self.true_positives + self.false_negatives > 0 {
            self.true_positives as f64 / (self.true_positives + self.false_negatives) as f64
        } else {
            0.0
        }
    }

    /// Aggregate F1 score across all evaluated items.
    pub fn f1(&self) -> f64 {
        let p = self.precision();
        let r = self.recall();
        if (p + r) > 0.0 {
            2.0 * p * r / (p + r)
        } else {
            0.0
        }
    }
}

impl AddAssign for Metrics {
    fn add_assign(&mut self, other: Self) {
        self.true_positives += other.true_positives;
        self.false_positives += other.false_positives;
        self.false_negatives += other.false_negatives;
        self.duration_secs += other.duration_secs;
    }
}
