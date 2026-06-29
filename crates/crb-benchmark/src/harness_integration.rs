use std::path::Path;
use anyhow::Result;
use crb_reporting::{GoldenCommentEntry, PrResult};

/// Evaluate a PR using the crb-harness library API.
///
/// Loads the diff from `{benchmark_dir}/diffs/{owner}_{repo}_{pr}.diff` and
/// calls `crb_harness::review_pr()` directly.
///
/// For now, returns a basic `PrResult` with zero findings. Full integration
/// with agent orchestration will follow in a later iteration.
pub fn evaluate_pr(
    pr: &GoldenCommentEntry,
    benchmark_dir: &Path,
    _model: &str,
    _judge_model: &str,
    _max_findings: usize,
) -> Result<PrResult> {
    // Extract owner, repo, pr_num from URL
    let (_owner, _repo, _pr_num) = crate::scaffold::parse_github_url(&pr.url)?;

    // Load diff from benchmark_dir/diffs/
    let _diff = load_cached_diff(benchmark_dir, &_owner, &_repo, _pr_num)
        .unwrap_or_default();

    // For now, return a basic PrResult (full integration uses the harness library).
    // In the future this will call crb_harness::review_pr().

    use crb_judge::Metrics;

    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: 0,
        golden_count: pr.comments.len(),
        metrics: Metrics::default(),
        verdicts: vec![],
        cost: None,
    })
}

/// Load a cached diff file from the benchmark directory.
///
/// Diffs are stored at `{benchmark_dir}/diffs/{owner}_{repo}_{pr_num}.diff`.
fn load_cached_diff(
    benchmark_dir: &Path,
    owner: &str,
    repo: &str,
    pr_num: u32,
) -> Option<String> {
    let diffs_dir = benchmark_dir.join("diffs");
    let diff_path = diffs_dir.join(format!("{}_{}_{}.diff", owner, repo, pr_num));
    match std::fs::read_to_string(&diff_path) {
        Ok(content) => {
            tracing::info!(
                "Loaded cached diff ({} bytes) from {}",
                content.len(),
                diff_path.display()
            );
            Some(content)
        }
        Err(e) => {
            tracing::warn!(
                "Cached diff not found at {}: {}. Using empty diff.",
                diff_path.display(),
                e
            );
            None
        }
    }
}
