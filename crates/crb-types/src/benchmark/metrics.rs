use std::ops::AddAssign;

use serde::{Deserialize, Serialize};

/// Metrics for evaluation of a PR.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metrics {
    /// Total true positives.
    #[serde(alias = "total_tp")]
    pub true_positives: usize,

    /// Total false positives.
    #[serde(alias = "total_fp")]
    pub false_positives: usize,

    /// Total false negatives.
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

impl MetricsProvider for Vec<Metrics> {
    fn true_positives(&self) -> usize {
        self.iter().map(|m| m.true_positives()).sum()
    }

    fn false_positives(&self) -> usize {
        self.iter().map(|m| m.false_positives()).sum()
    }

    fn false_negatives(&self) -> usize {
        self.iter().map(|m| m.false_negatives()).sum()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_default() {
        let m = Metrics::default();
        insta::assert_debug_snapshot!(m);
    }

    #[test]
    fn test_metrics_precision_zero_division() {
        // tp + fp = 0 => precision returns 0.0
        let m = Metrics::default();
        insta::assert_debug_snapshot!(m.precision());

        let m2 = Metrics {
            true_positives: 0,
            false_positives: 0,
            false_negatives: 5,
            duration_secs: 0.0,
        };
        insta::assert_debug_snapshot!(m2.precision());
    }

    #[test]
    fn test_metrics_recall_zero_division() {
        // tp + fn = 0 => recall returns 0.0
        let m = Metrics::default();
        insta::assert_debug_snapshot!(m.recall());

        let m2 = Metrics {
            true_positives: 0,
            false_positives: 5,
            false_negatives: 0,
            duration_secs: 0.0,
        };
        insta::assert_debug_snapshot!(m2.recall());
    }

    #[test]
    fn test_metrics_f1_zero_division() {
        // precision + recall = 0 => f1 returns 0.0
        let m = Metrics::default();
        insta::assert_debug_snapshot!(m.f1());

        let m2 = Metrics {
            true_positives: 0,
            false_positives: 0,
            false_negatives: 5,
            duration_secs: 0.0,
        };
        insta::assert_debug_snapshot!(m2.f1());
    }

    #[test]
    fn test_metrics_add_assign() {
        let mut a = Metrics {
            true_positives: 5,
            false_positives: 2,
            false_negatives: 1,
            duration_secs: 10.0,
        };
        let b = Metrics {
            true_positives: 3,
            false_positives: 1,
            false_negatives: 2,
            duration_secs: 20.0,
        };
        a += b;
        insta::assert_debug_snapshot!(a);
    }

    #[test]
    fn test_metrics_provider_trait() {
        let m = Metrics {
            true_positives: 10,
            false_positives: 5,
            false_negatives: 3,
            duration_secs: 0.0,
        };
        insta::assert_debug_snapshot!((
            m.true_positives(),
            m.false_positives(),
            m.false_negatives()
        ));

        insta::assert_debug_snapshot!((m.precision()));
        insta::assert_debug_snapshot!((m.recall()));
        insta::assert_debug_snapshot!((m.f1()));
    }
}
