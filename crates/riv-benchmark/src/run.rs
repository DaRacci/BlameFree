use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use mti::prelude::{MagicTypeId, MagicTypeIdExt, V7};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use rig_core::providers::openrouter;
use riv_agents::agent::AgentEntry;
use riv_harness::{
    eval::{EvalConfig, EvalContext, EvalStrategy},
    review::review_diff,
};
use riv_reporting::cost::AnalyticsTracker;
use riv_shared::{diff::Diff, string::sanitize_filename, url::parse_github_url};
use riv_types::{
    RunEvent,
    benchmark::{
        golden::{GoldenComment, GoldenCommentEntry},
        result::PrResult,
    },
    capabilities::ReasoningEffort,
    cost::{AnalyticsSnapshot, CacheUsage, SessionUsage},
    finding::Finding,
    vcs::{
        pr::PrMeta,
        repository::{RemoteRepositoryMeta, VCSPlatform},
    },
    wrappers::Model,
};
use tokio::{sync::broadcast::Sender, task::JoinSet};
use tracing::{info, warn};

use crate::{diffs, scaffold};

#[derive(Debug, Clone)]
pub struct BenchmarkPrOutcome {
    pub pr_title: String,
    pub pr_url: String,
    pub pr_result: PrResult,
    pub analytics: AnalyticsSnapshot,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub struct BenchmarkRunResult {
    pub outcomes: Vec<BenchmarkPrOutcome>,
    pub analytics: AnalyticsSnapshot,
    pub duration: Duration,
    pub finding_count: usize,
}

impl BenchmarkRunResult {
    pub fn completed_results(&self) -> usize {
        self.outcomes.len()
    }
}

struct OrderedBenchmarkPrOutcome {
    order: usize,
    outcome: BenchmarkPrOutcome,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_benchmark(
    review_id: MagicTypeId,
    dataset_dir: &Path,
    benchmark_dir: &Path,
    selected_entries: Vec<GoldenCommentEntry>,
    model: Model,
    agents: &'static [&'static AgentEntry],
    reasoning_effort: Option<ReasoningEffort>,
    client: Arc<openrouter::Client>,
    dashboard_tx: Option<Sender<RunEvent>>,
    repo_root_fallback: PathBuf,
    max_findings: usize,
) -> Result<BenchmarkRunResult, String> {
    info!(
        "Running benchmark review {} across {} PR(s)",
        review_id,
        selected_entries.len()
    );

    prepare_benchmark_assets(dataset_dir, benchmark_dir).await;

    let mut pr_set = JoinSet::new();
    for (order, entry) in selected_entries.into_iter().enumerate() {
        let review_id = review_id.clone();
        let model = model.clone();
        let reasoning_effort = reasoning_effort.clone();
        let client = client.clone();
        let dashboard_tx = dashboard_tx.clone();
        let benchmark_dir = benchmark_dir.to_path_buf();
        let repo_root_fallback = repo_root_fallback.clone();

        pr_set.spawn(async move {
            run_benchmark_pr(
                order,
                review_id,
                entry,
                model,
                agents,
                reasoning_effort,
                client,
                dashboard_tx,
                benchmark_dir,
                repo_root_fallback,
                max_findings,
            )
            .await
        });
    }

    let mut ordered_outcomes = Vec::new();
    while let Some(join_result) = pr_set.join_next().await {
        match join_result {
            Ok(Ok(outcome)) => ordered_outcomes.push(outcome),
            Ok(Err(error)) => warn!("Benchmark PR evaluation failed: {error}"),
            Err(error) => warn!("Benchmark PR task join error: {error}"),
        }
    }

    ordered_outcomes.sort_by_key(|outcome| outcome.order);

    let mut outcomes = Vec::with_capacity(ordered_outcomes.len());
    let mut analytics = AnalyticsSnapshot::default();
    let mut duration = Duration::default();
    let mut finding_count = 0usize;

    for ordered in ordered_outcomes {
        merge_analytics(&mut analytics, &ordered.outcome.analytics);
        duration += ordered.outcome.duration;
        finding_count += ordered.outcome.pr_result.findings.len();
        outcomes.push(ordered.outcome);
    }

    Ok(BenchmarkRunResult {
        outcomes,
        analytics,
        duration,
        finding_count,
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_benchmark_pr(
    order: usize,
    review_id: MagicTypeId,
    entry: GoldenCommentEntry,
    model: Model,
    agents: &'static [&'static AgentEntry],
    reasoning_effort: Option<ReasoningEffort>,
    client: Arc<openrouter::Client>,
    dashboard_tx: Option<Sender<RunEvent>>,
    benchmark_dir: PathBuf,
    repo_root_fallback: PathBuf,
    max_findings: usize,
) -> Result<OrderedBenchmarkPrOutcome, String> {
    let started_at = Instant::now();
    let (owner, repo, pr_number) = parse_github_url(&entry.url)
        .map_err(|error| format!("Invalid dataset PR URL {}: {error}", entry.url))?;

    let diff = match load_cached_benchmark_diff(&benchmark_dir, &owner, &repo, pr_number) {
        Some(diff) if !diff.trim().is_empty() => diff,
        _ => fetch_pr_diff(&owner, &repo, pr_number)
            .await
            .map_err(|error| format!("Failed to fetch diff for {}: {error}", entry.url))?,
    };

    let repo_root = benchmark_worktree_dir(&benchmark_dir, &owner, &repo, pr_number);
    let repo_root = if repo_root.exists() {
        repo_root
    } else {
        repo_root_fallback
    };

    let pr_meta = PrMeta {
        title: entry.pr_title.clone(),
        url: entry.url.clone(),
        number: pr_number,
    };
    let repository = RemoteRepositoryMeta {
        platform: VCSPlatform::GitHub,
        owner,
        name: repo,
    };
    let cost_tracker = Arc::new(AnalyticsTracker::new());
    let config = EvalConfig {
        review_id: review_id.clone(),
        context: EvalContext {
            repo_root,
            ruleset: None,
            repository,
            pull_request: Some(pr_meta),
        },
        strategy: EvalStrategy::Panel,
        model,
        reasoning_effort,
        client,
        cache: None,
        cost_tracker: cost_tracker.clone(),
        dashboard_tx,
        agents,
        max_findings,
        template_vars: None,
    };

    let findings = review_diff(Diff::new(diff), &config)
        .await
        .map_err(|error| format!("Benchmark evaluation failed for {}: {error}", entry.url))?;
    let analytics = cost_tracker.to_snapshot().await;
    let duration = started_at.elapsed();

    let pr_result_id = make_pr_result_id(&review_id, &entry.url, &entry.pr_title);
    let pr_result = build_pr_result(pr_result_id, Some(review_id), entry.comments, findings);

    Ok(OrderedBenchmarkPrOutcome {
        order,
        outcome: BenchmarkPrOutcome {
            pr_title: entry.pr_title,
            pr_url: entry.url,
            pr_result,
            analytics,
            duration,
        },
    })
}

async fn prepare_benchmark_assets(dataset_dir: &Path, benchmark_dir: &Path) {
    let dataset_dir = dataset_dir.to_path_buf();
    let scaffold_dir = benchmark_dir.to_path_buf();
    match tokio::task::spawn_blocking(move || scaffold::run(&dataset_dir, &scaffold_dir)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(
            "Benchmark scaffold failed; falling back to live diff fetch: {}",
            error
        ),
        Err(error) => warn!(
            "Benchmark scaffold task failed; falling back to live diff fetch: {}",
            error
        ),
    }

    let diff_dir = benchmark_dir.to_path_buf();
    match tokio::task::spawn_blocking(move || diffs::run(&diff_dir)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(
            "Benchmark diff extraction failed; falling back to live diff fetch: {}",
            error
        ),
        Err(error) => warn!(
            "Benchmark diff extraction task failed; falling back to live diff fetch: {}",
            error
        ),
    }
}

fn build_pr_result(
    id: MagicTypeId,
    benchmark_id: Option<MagicTypeId>,
    golden_comments: Vec<GoldenComment>,
    findings: Vec<Finding>,
) -> PrResult {
    let pr_result_id = id.to_string();
    let golden_comments = golden_comments
        .into_iter()
        .map(|mut comment| {
            comment.pr_result_id = Some(id.clone());
            comment
        })
        .collect();
    let findings = findings
        .into_iter()
        .map(|mut finding| {
            finding.pr_result_id = pr_result_id.clone();
            finding
        })
        .collect();

    PrResult {
        id,
        benchmark_id,
        golden_comments,
        findings,
    }
}

fn load_cached_benchmark_diff(
    benchmark_dir: &Path,
    owner: &str,
    repo: &str,
    pr_number: u32,
) -> Option<String> {
    let diff_path = benchmark_dir
        .join("diffs")
        .join(format!("{owner}_{repo}_{pr_number}.diff"));
    fs::read_to_string(diff_path).ok()
}

fn benchmark_worktree_dir(
    benchmark_dir: &Path,
    owner: &str,
    repo: &str,
    pr_number: u32,
) -> PathBuf {
    benchmark_dir
        .join("worktrees")
        .join(format!("{owner}_{repo}_{pr_number}"))
}

fn make_pr_result_id(review_id: &MagicTypeId, pr_url: &str, pr_title: &str) -> MagicTypeId {
    let key = parse_github_url(pr_url)
        .map(|(owner, repo, number)| format!("{owner}-{repo}-{number}"))
        .unwrap_or_else(|_| sanitize_filename(pr_title));
    format!("{review_id}/{key}").create_type_id::<V7>()
}

fn merge_analytics(target: &mut AnalyticsSnapshot, source: &AnalyticsSnapshot) {
    for (session_id, usage) in &source.sessions {
        let entry = target.sessions.entry(session_id.clone()).or_default();
        merge_session_usage(entry, usage);
    }
    for (session_id, usage) in &source.cache_usage {
        let entry = target.cache_usage.entry(session_id.clone()).or_default();
        merge_cache_usage(entry, usage);
    }
}

fn merge_session_usage(target: &mut SessionUsage, source: &SessionUsage) {
    target.input_tokens += source.input_tokens;
    target.output_tokens += source.output_tokens;
    target.cached_input_tokens += source.cached_input_tokens;
    target.cache_creation_input_tokens += source.cache_creation_input_tokens;
    target.reasoning_tokens += source.reasoning_tokens;
    target.tool_use_prompt_tokens += source.tool_use_prompt_tokens;
    target.call_count += source.call_count;
    target.tool_use_count += source.tool_use_count;
}

fn merge_cache_usage(target: &mut CacheUsage, source: &CacheUsage) {
    target.cache_hits += source.cache_hits;
    target.cache_misses += source.cache_misses;
}

async fn fetch_pr_diff(owner: &str, repo: &str, pr_number: u32) -> Result<String, String> {
    let mut request = reqwest::Client::new()
        .get(format!(
            "https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}"
        ))
        .header(ACCEPT, "application/vnd.github.v3.diff")
        .header(USER_AGENT, "blamefree-benchmark/1.0");

    if let Ok(token) = env::var("GITHUB_TOKEN") {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }

    request
        .send()
        .await
        .map_err(|error| format!("Failed to fetch PR diff: {error}"))?
        .text()
        .await
        .map_err(|error| format!("Failed to read diff text: {error}"))
}
