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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_default() {
        let m = Metrics::default();
        insta::assert_debug_snapshot!(m);
    }

    #[test]
    fn test_metrics_aliases_deserialize() {
        let json = r#"{"total_tp":5,"total_fp":2,"total_fn":1,"duration_secs":10.0}"#;
        let m: Metrics = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(m);

        // Also verify the "elapsed" alias for duration_secs
        let json2 = r#"{"true_positives":1,"false_positives":0,"false_negatives":0,"elapsed":30.0}"#;
        let m2: Metrics = serde_json::from_str(json2).unwrap();
        insta::assert_debug_snapshot!(m2);
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
        insta::assert_debug_snapshot!((m.true_positives(), m.false_positives(), m.false_negatives()));

        let tp = 10.0_f64;
        let fp = 5.0_f64;
        let fn_val = 3.0_f64;
        let expected_precision = tp / (tp + fp);
        let expected_recall = tp / (tp + fn_val);
        let expected_f1 = 2.0 * expected_precision * expected_recall
            / (expected_precision + expected_recall);

        insta::assert_debug_snapshot!((m.precision(), expected_precision));
        insta::assert_debug_snapshot!((m.recall(), expected_recall));
        insta::assert_debug_snapshot!((m.f1(), expected_f1));
    }

    #[test]
    fn test_judge_verdict_serde_roundtrip() {
        let original = JudgeVerdict {
            reasoning: "Found a semantic match".into(),
            match_: true,
            confidence: 0.95,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: JudgeVerdict = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&original);

        // Also test deserializing from JSON with "match" key
        let json2 = r#"{"reasoning":"No match","match":false,"confidence":0.3}"#;
        let v: JudgeVerdict = serde_json::from_str(json2).unwrap();
        insta::assert_debug_snapshot!(v);
    }

    #[test]
    fn test_judge_verdict_default() {
        let v = JudgeVerdict {
            reasoning: String::new(),
            match_: false,
            confidence: 0.0,
        };
        insta::assert_debug_snapshot!(v);
    }
}
