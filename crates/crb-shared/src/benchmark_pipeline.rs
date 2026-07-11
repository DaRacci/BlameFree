//! Shared benchmark evaluation pipeline.
//!
//! Provides the core concurrent evaluation loop, metrics aggregation, and
//! helper traits so that callers (`crb-benchmark`, `crb-webui-backend`) can
//! share the same pipeline structure without duplicating the spawning,
//! result-collection, and aggregation logic.
//!
//! # Circular-dependency constraint
//!
//! `crb-shared` must NOT depend on crates that themselves depend on
//! `crb-shared` (i.e. `crb-harness`, `crb-reporting`, `crb-tools`,
//! `crb-agents`).  To work around this, the shared pipeline uses **traits**
//! (`HasEvalMetrics`, `HasUrl`) which are implemented *by* those downstream
//! crates for their concrete types.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// A type that can provide evaluation metrics for result aggregation.
pub trait HasEvalMetrics {
    fn true_positives(&self) -> usize;
    fn false_positives(&self) -> usize;
    fn false_negatives(&self) -> usize;
    fn cost_usd(&self) -> f64;
    fn total_tokens(&self) -> usize;
    fn agent_calls(&self) -> usize;
}

/// A type that exposes a PR/issue URL string.
pub trait HasUrl {
    fn url(&self) -> &str;
}

// ── Configuration ───────────────────────────────────────────────────────────

/// Configuration for the concurrent PR evaluation loop.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Maximum number of concurrent PR evaluations.
    pub concurrency: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self { concurrency: 4 }
    }
}

impl PipelineConfig {
    /// Create a new pipeline config with the given concurrency limit.
    pub fn new(concurrency: usize) -> Self {
        Self { concurrency }
    }
}

// ── Aggregate results ───────────────────────────────────────────────────────

/// Accumulated aggregate results from running the evaluation pipeline.
///
/// Callers push individual results via [`add`](Self::add) and then call
/// [`precision`](Self::precision) / [`recall`](Self::recall) / [`f1`](Self::f1)
/// for the final summary.
#[derive(Debug, Clone, Default)]
pub struct AggregateResults {
    /// Total true positives across all evaluated items.
    pub total_tp: usize,
    /// Total false positives across all evaluated items.
    pub total_fp: usize,
    /// Total false negatives across all evaluated items.
    pub total_fn: usize,
    /// Total cost in USD across all evaluated items.
    pub total_cost: f64,
    /// Total tokens consumed across all evaluated items.
    pub total_tokens: usize,
    /// Total number of agent API calls made.
    pub total_agent_calls: usize,
    /// Wall-clock duration of the evaluation run.
    pub elapsed: Duration,
}

impl AggregateResults {
    /// Create a new empty aggregate results accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a single PR's metrics into the aggregate totals.
    pub fn add<R: HasEvalMetrics>(&mut self, r: &R) {
        self.total_tp += r.true_positives();
        self.total_fp += r.false_positives();
        self.total_fn += r.false_negatives();
        self.total_cost += r.cost_usd();
        self.total_tokens += r.total_tokens();
        self.total_agent_calls += r.agent_calls();
    }

    /// Aggregate precision across all evaluated items.
    pub fn precision(&self) -> f64 {
        if self.total_tp + self.total_fp > 0 {
            self.total_tp as f64 / (self.total_tp + self.total_fp) as f64
        } else {
            0.0
        }
    }

    /// Aggregate recall across all evaluated items.
    pub fn recall(&self) -> f64 {
        if self.total_tp + self.total_fn > 0 {
            self.total_tp as f64 / (self.total_tp + self.total_fn) as f64
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

// ── Concurrent evaluation loop ─────────────────────────────────────────────-

/// Run a concurrent evaluation loop over a list of items.
///
/// Spawns up to `config.concurrency` tasks at a time, each calling `eval_fn`.
/// Returns the collected results (failed evaluations are logged and skipped)
/// and the wall-clock duration.
///
/// # Type parameters
///
/// * `T` — each item to evaluate (must be `Send + 'static`).
/// * `R` — the result type returned by each evaluation (`Send + 'static`).
/// * `F` — an async closure/fn: `Fn(T) -> Fut`.
pub async fn run_concurrent_eval<T, R, F, Fut>(
    items: Vec<T>,
    config: &PipelineConfig,
    eval_fn: Arc<F>,
) -> (Vec<R>, Duration)
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = anyhow::Result<R>> + Send,
{
    let total = items.len();
    let sem = Arc::new(Semaphore::new(config.concurrency));
    let mut set = JoinSet::new();
    let start = Instant::now();

    for item in items {
        let permit = sem
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let eval_fn = eval_fn.clone();
        set.spawn(async move {
            let _permit = permit;
            eval_fn(item).await
        });
    }

    let mut results = Vec::with_capacity(total.min(config.concurrency));
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => tracing::error!("PR evaluation failed: {e}"),
            Err(e) => tracing::error!("Join error: {e}"),
        }
    }

    (results, start.elapsed())
}

// ── PR filtering ────────────────────────────────────────────────────────────

/// Filter a list of PR entries by a comma-separated filter string.
///
/// Supports several match modes:
/// * `owner/repo/N` — exact PR number within a repository.
/// * `N` — bare number, matched exactly against the PR number portion of the
///   URL.
/// * `owner/repo/pull/1` — exact URL suffix.
/// * Any substring that appears in the lowercased URL.
///
/// When no PRs match, a warning is logged with available URLs.
pub fn filter_prs_by_pattern<T>(all_prs: Vec<T>, filter: &str) -> Vec<T>
where
    T: HasUrl + Clone,
{
    let filter_patterns: std::collections::HashSet<String> =
        filter.split(',').map(|s| s.trim().to_lowercase()).collect();

    let available_urls: Vec<String> = all_prs.iter().map(|pr| pr.url().to_string()).collect();

    let filtered: Vec<_> = all_prs
        .into_iter()
        .filter(|pr| {
            let url_lower = pr.url().to_lowercase();
            filter_patterns.iter().any(|pattern| {
                // Parse pattern as "repo/N" where N is a PR number
                if let Some((repo_part, pr_num_str)) = pattern.split_once('/') {
                    if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                        // Exact PR number match: `/pull/N` must NOT be followed by a digit
                        let pr_tag = format!("/pull/{pr_num}");
                        if let Some(pos) = url_lower.find(&pr_tag) {
                            let after = &url_lower[pos + pr_tag.len()..];
                            if after.is_empty() || !after.chars().next().unwrap().is_ascii_digit()
                            {
                                if url_lower.contains(repo_part) {
                                    return true;
                                }
                            }
                        }
                    }
                }
                // Exact match only — avoid substring bugs where "1" matches "/pull/10".
                if let Ok(num) = pattern.parse::<u32>() {
                    // Bare number: match exactly against the PR number from the URL.
                    url_lower
                        .rsplit('/')
                        .next()
                        .and_then(|s| s.parse::<u32>().ok())
                        == Some(num)
                } else {
                    // Non-numeric fallback: exact URL suffix match (e.g. "repo/pull/1").
                    url_lower.ends_with(&format!("/{pattern}"))
                }
            })
        })
        .collect();

    if filtered.is_empty() {
        tracing::warn!(
            "PR filter \"{filter}\" matched no PRs. Available URLs:\n  {}",
            available_urls.join("\n  ")
        );
    }

    filtered
}
