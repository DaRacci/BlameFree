use serde::{Deserialize, Serialize};

/// Aggregate metrics computed from total true/false positives and false negatives.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MetricsOutput {
    /// Precision score (tp / (tp + fp)).
    pub precision: f64,
    /// Recall score (tp / (tp + fn)).
    pub recall: f64,
    /// F1 score (harmonic mean of precision and recall).
    pub f1: f64,
}

/// Compute precision, recall, and F₁ score from aggregate counts.
///
/// Each metric includes zero-division protection, returning `0.0` when the
/// denominator is zero.
///
/// # Formulas
///
/// * precision = tp / (tp + fp)
/// * recall    = tp / (tp + fn)
/// * f1       = 2 * precision * recall / (precision + recall)
///
/// # Returns
///
/// A [`MetricsOutput`] with the computed precision, recall, and F1 values.
pub fn compute_aggregate_metrics(
    total_tp: usize,
    total_fp: usize,
    total_fn: usize,
) -> MetricsOutput {
    let precision = if total_tp + total_fp > 0 {
        total_tp as f64 / (total_tp + total_fp) as f64
    } else {
        0.0
    };
    let recall = if total_tp + total_fn > 0 {
        total_tp as f64 / (total_tp + total_fn) as f64
    } else {
        0.0
    };
    let f1 = if (precision + recall) > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    MetricsOutput {
        precision,
        recall,
        f1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_case() {
        let m = compute_aggregate_metrics(10, 5, 2);
        let expected_p = 10.0 / 15.0;
        let expected_r = 10.0 / 12.0;
        let expected_f1 = 2.0 * expected_p * expected_r / (expected_p + expected_r);
        assert!((m.precision - expected_p).abs() < 1e-12);
        assert!((m.recall - expected_r).abs() < 1e-12);
        assert!((m.f1 - expected_f1).abs() < 1e-12);
    }

    #[test]
    fn perfect() {
        let m = compute_aggregate_metrics(10, 0, 0);
        assert!((m.precision - 1.0).abs() < 1e-12);
        assert!((m.recall - 1.0).abs() < 1e-12);
        assert!((m.f1 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn zero_tp() {
        let m = compute_aggregate_metrics(0, 5, 3);
        assert_eq!(m.precision, 0.0);
        assert_eq!(m.recall, 0.0);
        assert_eq!(m.f1, 0.0);
    }

    #[test]
    fn all_zero() {
        let m = compute_aggregate_metrics(0, 0, 0);
        assert_eq!(m.precision, 0.0);
        assert_eq!(m.recall, 0.0);
        assert_eq!(m.f1, 0.0);
    }

    #[test]
    fn zero_fp() {
        let m = compute_aggregate_metrics(7, 0, 3);
        assert!((m.precision - 1.0).abs() < 1e-12);
        assert!((m.recall - 7.0 / 10.0).abs() < 1e-12);
        assert!(m.f1 > 0.0);
    }

    #[test]
    fn struct_implements_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<MetricsOutput>();
        assert_sync::<MetricsOutput>();
    }
}
