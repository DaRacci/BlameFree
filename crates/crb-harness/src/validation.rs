use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde::Serialize;

use crb_judge::Metrics;

/// Baseline expectations for a specific version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    /// Version label (e.g. "5.14").
    pub version: String,

    /// Expected metric values at this version.
    pub expected: ExpectedMetrics,

    /// Allowed deltas (absolute deviation) for each metric.
    pub thresholds: MetricThresholds,
}

/// Expected metric values that the benchmark should achieve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedMetrics {
    pub total_prs: usize,
    pub avg_precision: f64,
    pub avg_recall: f64,
    pub avg_f1: f64,
}

/// Tolerance thresholds for each metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricThresholds {
    pub precision_delta: f64,
    pub recall_delta: f64,
    pub f1_delta: f64,
}

/// Result of comparing computed metrics against a baseline.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub in_threshold: bool,
    pub deltas: MetricDeltas,
    pub total_prs: usize,
}

/// Deltas (computed - expected) for each metric.
#[derive(Debug, Clone)]
pub struct MetricDeltas {
    pub precision_delta: f64,
    pub recall_delta: f64,
    pub f1_delta: f64,
}

/// Load a baseline JSON file from the given path.
///
/// The file is expected at `<workspace_root>/baselines/<version>.json`.
pub fn load_baseline(workspace_root: &Path, version: &str) -> Result<Baseline> {
    let path = workspace_root
        .join("baselines")
        .join(format!("{version}.json"));
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read baseline file: {}", path.display()))?;
    let baseline: Baseline = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse baseline JSON from: {}", path.display()))?;
    Ok(baseline)
}

/// Compute average metrics across a set of individual PR results.
pub fn compute_average_metrics(results: &[Metrics]) -> (f64, f64, f64) {
    if results.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let n = results.len() as f64;
    let avg_precision = results.iter().map(|m| m.precision).sum::<f64>() / n;
    let avg_recall = results.iter().map(|m| m.recall).sum::<f64>() / n;
    let avg_f1 = results.iter().map(|m| m.f1).sum::<f64>() / n;
    (avg_precision, avg_recall, avg_f1)
}

/// Compare computed average metrics against a baseline.
///
/// Returns a `ValidationResult` indicating whether all metrics are within
/// their respective thresholds, along with the computed deltas.
pub fn validate_against_baseline(
    baseline: &Baseline,
    total_prs: usize,
    avg_precision: f64,
    avg_recall: f64,
    avg_f1: f64,
) -> ValidationResult {
    let precision_delta = (avg_precision - baseline.expected.avg_precision).abs();
    let recall_delta = (avg_recall - baseline.expected.avg_recall).abs();
    let f1_delta = (avg_f1 - baseline.expected.avg_f1).abs();

    let in_threshold = precision_delta <= baseline.thresholds.precision_delta
        && recall_delta <= baseline.thresholds.recall_delta
        && f1_delta <= baseline.thresholds.f1_delta;

    ValidationResult {
        in_threshold,
        deltas: MetricDeltas {
            precision_delta,
            recall_delta,
            f1_delta,
        },
        total_prs,
    }
}

/// Format and print a validation summary to stdout.
pub fn print_validation_summary(
    baseline: &Baseline,
    result: &ValidationResult,
    avg_precision: f64,
    avg_recall: f64,
    avg_f1: f64,
) {
    println!("═══════════════════════════════════════════");
    println!("  Baseline Validation Report");
    println!("  Version:           {}", baseline.version);
    println!("  Total PRs:         {}", result.total_prs);
    println!("───────────────────────────────────────────");
    println!("  Metric        Expected    Actual    Delta  Threshold  Status");
    println!("  ──────        ────────    ──────    ─────  ─────────  ──────");
    print_metric_line(
        "Precision",
        baseline.expected.avg_precision,
        avg_precision,
        result.deltas.precision_delta,
        baseline.thresholds.precision_delta,
    );
    print_metric_line(
        "Recall",
        baseline.expected.avg_recall,
        avg_recall,
        result.deltas.recall_delta,
        baseline.thresholds.recall_delta,
    );
    print_metric_line(
        "F1",
        baseline.expected.avg_f1,
        avg_f1,
        result.deltas.f1_delta,
        baseline.thresholds.f1_delta,
    );
    println!("───────────────────────────────────────────");
    if result.in_threshold {
        println!("  ✅ ALL METRICS WITHIN THRESHOLDS");
    } else {
        println!("  ❌ METRICS EXCEED THRESHOLDS");
    }
    println!("═══════════════════════════════════════════");
}

fn print_metric_line(name: &str, expected: f64, actual: f64, delta: f64, threshold: f64) {
    let status = if delta <= threshold { "✅" } else { "❌" };
    println!(
        "  {:<14} {:<10.4} {:<9.4} {:<7.4} {:<10.4} {}",
        name, expected, actual, delta, threshold, status
    );
}
