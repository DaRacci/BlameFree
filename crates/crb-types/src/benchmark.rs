use std::ops::AddAssign;

use serde::{Deserialize, Serialize};

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

pub trait MetricsProvider {
    fn true_positives(&self) -> usize;
    fn false_positives(&self) -> usize;
    fn false_negatives(&self) -> usize;

    /// Aggregate precision across all evaluated items.
    fn precision(&self) -> f64 {
        if self.true_positives() + self.false_positives() > 0 {
            self.true_positives() as f64 / (self.true_positives() + self.false_positives()) as f64
        } else {
            0.0
        }
    }

    /// Aggregate recall across all evaluated items.
    fn recall(&self) -> f64 {
        if self.true_positives() + self.false_negatives() > 0 {
            self.true_positives() as f64 / (self.true_positives() + self.false_negatives()) as f64
        } else {
            0.0
        }
    }

    /// Aggregate F1 score across all evaluated items.
    fn f1(&self) -> f64 {
        let p = self.precision();
        let r = self.recall();
        if (p + r) > 0.0 {
            2.0 * p * r / (p + r)
        } else {
            0.0
        }
    }
}

impl MetricsProvider for Metrics {
    fn true_positives(&self) -> usize {
        self.true_positives
    }

    fn false_positives(&self) -> usize {
        self.false_positives
    }

    fn false_negatives(&self) -> usize {
        self.false_negatives
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

/// The structured verdict returned by the judge LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeVerdict {
    /// Brief explanation of why the judge determined a match or no match.
    pub reasoning: String,

    /// Whether the candidate finding matches the golden comment.
    #[serde(rename = "match")]
    pub match_: bool,

    /// Confidence level for this judgment
    pub confidence: f32,
}
