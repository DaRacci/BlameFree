use std::{
    any::Any,
    collections::HashSet,
    env, fs,
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use futures::FutureExt;
use mti::prelude::{MagicTypeId, MagicTypeIdExt, V7};
use rig_core::{client::ProviderClient, providers::openrouter};
use riv_agents::{agent::AgentEntry, prompts::PromptLibrary};
use riv_benchmark::{BENCHMARK_DIR, run_benchmark};
use riv_harness::{
    eval::{EvalConfig, EvalContext, EvalStrategy},
    model_capabilities::{available_models, supports_reasoning},
    review::review_diff,
};
use riv_reporting::{cost::AnalyticsTracker, golden::load_golden_datasets};
use riv_shared::{diff::Diff, url::parse_github_url};
use riv_stor::traits::Store;
use riv_types::{
    RunEvent,
    agent::{AgentSession, AgentTurn, AgentTurnMessage, RoleMessage, ToolInvocation},
    benchmark::{
        golden::{GoldenComment, GoldenCommentEntry},
        result::PrResult,
    },
    capabilities::ReasoningEffort,
    finding::Finding,
    review::{PullRequestReviewMetadata, Review, ReviewMetadata, ReviewStatus},
    vcs::{
        pr::PrMeta,
        repository::{RemoteRepositoryMeta, VCSPlatform},
    },
    wrappers::Model,
};
use riv_webui_app::LiveAgentInfo;
use riv_webui_shared::{
    config::{AgentInfo, DatasetInfo},
    review::ReviewAgentLog,
};
use tokio::sync::broadcast;
use tracing::{error, warn};

use crate::server::AppState;

const MAX_FINDINGS_PER_AGENT: usize = 20;
const REVIEW_ID_PREFIX: &str = "review";
const BENCHMARK_REVIEW_ID_PREFIX: &str = "benchmark";

pub async fn list_reviews<S>(state: &AppState<S>) -> Result<Vec<Review>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let mut reviews = state
        .store
        .list::<Review>(&())
        .await
        .map_err(|error| format!("Failed to list reviews: {error}"))?;

    for review in &mut reviews {
        reconcile_review_state(state, review).await;
    }

    reviews.sort_by(|left, right| {
        let left_active = matches!(left.status, ReviewStatus::Pending | ReviewStatus::Running);
        let right_active = matches!(right.status, ReviewStatus::Pending | ReviewStatus::Running);

        right_active
            .cmp(&left_active)
            .then_with(|| right.id.to_string().cmp(&left.id.to_string()))
    });

    Ok(reviews)
}

pub async fn get_review<S>(state: &AppState<S>, review_id: &MagicTypeId) -> Result<Review, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let mut review = state
        .store
        .load::<Review>(review_id)
        .await
        .map_err(|error| format!("Failed to load review {review_id}: {error}"))?
        .ok_or_else(|| format!("Review {review_id} not found"))?;

    reconcile_review_state(state, &mut review).await;
    Ok(review)
}

pub async fn list_pr_results<S>(
    state: &AppState<S>,
    review_id: &MagicTypeId,
) -> Result<Vec<PrResult>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let review_key = review_id.to_string();
    let mut results = state
        .store
        .list::<PrResult>(&())
        .await
        .map_err(|error| format!("Failed to list PR results: {error}"))?
        .into_iter()
        .filter(|result| result_matches_review(result, review_id, &review_key))
        .collect::<Vec<_>>();

    results.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
    Ok(results)
}

pub async fn list_agent_logs<S>(
    state: &AppState<S>,
    review_id: &MagicTypeId,
) -> Result<Vec<ReviewAgentLog>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let review = get_review(state, review_id).await?;
    let mut logs = review
        .agent_sessions
        .iter()
        .map(|session| build_review_agent_log(review_id, session))
        .collect::<Vec<_>>();

    logs.sort_by(|left, right| left.agent_id.to_string().cmp(&right.agent_id.to_string()));
    Ok(logs)
}

pub async fn list_live_review_agents<S>(
    state: &AppState<S>,
    review_id: &MagicTypeId,
) -> Result<Vec<LiveAgentInfo>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let live_review_agents = state.live_review_agents.read().await;
    Ok(live_review_agents
        .get(review_id)
        .cloned()
        .unwrap_or_default())
}

pub async fn list_repo_prs<S>(
    state: &AppState<S>,
    owner: &str,
    repo: &str,
) -> Result<Vec<PrMeta>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let page = state
        .octocrab
        .pulls(owner, repo)
        .list()
        .state(octocrab::params::State::Open)
        .per_page(100)
        .send()
        .await
        .map_err(|error| format!("GitHub API error: {error}"))?;

    Ok(page
        .items
        .into_iter()
        .map(|pr| PrMeta {
            number: u32::try_from(pr.number).unwrap_or(u32::MAX),
            title: pr.title.unwrap_or_default(),
            url: pr.html_url.map(|url| url.to_string()).unwrap_or_default(),
        })
        .collect())
}

pub async fn fetch_pr_diff<S>(
    state: &AppState<S>,
    owner: &str,
    repo: &str,
    pr_number: u32,
) -> Result<(String, String), String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let pr = state
        .octocrab
        .pulls(owner, repo)
        .get(u64::from(pr_number))
        .await
        .map_err(|error| format!("Failed to fetch PR metadata: {error}"))?;

    let title = pr.title.unwrap_or_default();

    let diff_client = reqwest::Client::new();
    let token = env::var("GITHUB_TOKEN").ok();
    let mut diff_request = diff_client
        .get(format!(
            "https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}"
        ))
        .header("Accept", "application/vnd.github.v3.diff")
        .header("User-Agent", "blamefree-webui/1.0");

    if let Some(token) = token {
        diff_request = diff_request.header("Authorization", format!("Bearer {token}"));
    }

    let diff = diff_request
        .send()
        .await
        .map_err(|error| format!("Failed to fetch PR diff: {error}"))?
        .text()
        .await
        .map_err(|error| format!("Failed to read diff text: {error}"))?;

    Ok((title, diff))
}

pub async fn list_datasets<S>(state: &AppState<S>) -> Result<Vec<DatasetInfo>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let dataset_dir = &state.config.server.dataset_dir;
    if !dataset_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(dataset_dir).map_err(|error| {
        format!(
            "Failed to read dataset directory {}: {error}",
            dataset_dir.display()
        )
    })?;

    let mut datasets = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            continue;
        }

        let pr_count = load_golden_datasets(&path)
            .map(|entries| entries.len())
            .unwrap_or_default();

        datasets.push(DatasetInfo {
            id,
            path: path.to_string_lossy().to_string(),
            pr_count,
        });
    }

    datasets.sort_by(|left, right| {
        right
            .pr_count
            .cmp(&left.pr_count)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(datasets)
}

pub async fn list_dataset_prs<S>(
    state: &AppState<S>,
    dataset_id: &str,
) -> Result<Vec<GoldenCommentEntry>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let dataset_dir = state.config.server.dataset_dir.join(dataset_id);
    if !dataset_dir.exists() || !dataset_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut prs = load_golden_datasets(&dataset_dir)
        .map_err(|error| format!("Failed to load dataset {dataset_id}: {error}"))?;

    prs.sort_by(|left, right| {
        extract_pr_number(&left.url)
            .cmp(&extract_pr_number(&right.url))
            .then_with(|| left.pr_title.cmp(&right.pr_title))
    });
    Ok(prs)
}

pub async fn list_models<S>(_state: &AppState<S>) -> Result<Vec<Model>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    Ok(available_models())
}

pub async fn list_reasoning_efforts<S>(
    _state: &AppState<S>,
    model: &str,
) -> Result<Vec<ReasoningEffort>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    if !supports_reasoning(&Model(model.to_string())) {
        return Ok(Vec::new());
    }

    Ok(vec![
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::XHigh,
        ReasoningEffort::Max,
    ])
}

pub async fn list_agents<S>(_state: &AppState<S>) -> Result<Vec<AgentInfo>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let library = PromptLibrary::new().map_err(|error| error.to_string())?;
    let mut abbreviations = library
        .abbreviations()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    abbreviations.sort();

    Ok(abbreviations
        .into_iter()
        .filter_map(|abbreviation| {
            library.config(&abbreviation).map(|entry| AgentInfo {
                name: entry.role_name.clone(),
                abbreviation,
                incompatible_with_roles: entry.incompatible_with_roles.clone(),
            })
        })
        .collect())
}

pub async fn start_review<S>(
    state: &AppState<S>,
    url: &str,
    model: &str,
    roles: &[String],
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<Review, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    ensure_openrouter_configured()?;

    let (owner, repo, pr_number) =
        parse_github_url(url).map_err(|error| format!("Invalid GitHub PR URL: {error}"))?;
    let (pr_title, diff) = fetch_pr_diff(state, &owner, &repo, pr_number).await?;

    let review_id = new_review_id(REVIEW_ID_PREFIX);
    let repository = RemoteRepositoryMeta {
        platform: VCSPlatform::GitHub,
        owner,
        name: repo,
    };
    let pr_meta = PrMeta {
        title: pr_title,
        url: url.to_string(),
        number: pr_number,
    };

    let live_agents = resolve_live_agents(roles)?;

    let review = Review {
        id: review_id.clone(),
        agent_sessions: Vec::new(),
        analytics: None,
        duration: None,
        status: ReviewStatus::Running,
        metadata: ReviewMetadata::PullRequest(PullRequestReviewMetadata {
            repository: repository.clone(),
            meta: pr_meta.clone(),
        }),
    };

    state
        .store
        .save::<Review>(&review)
        .await
        .map_err(|error| format!("Failed to save running review: {error}"))?;

    let dashboard_tx = register_live_review(state, &review_id, live_agents).await;
    let task_state = state.clone();
    let task_model = model.to_string();
    let task_roles = roles.to_vec();
    let failure_review = review.clone();
    let task_dashboard_tx = dashboard_tx.clone();
    tokio::spawn(async move {
        run_launch_job_with_recovery(
            task_state.clone(),
            failure_review,
            dashboard_tx,
            "Review",
            run_review_job(
                task_state,
                review_id,
                repository,
                pr_meta,
                diff,
                task_model,
                task_roles,
                reasoning_effort,
                task_dashboard_tx,
            ),
        )
        .await;
    });

    Ok(review)
}

pub async fn start_benchmark<S>(
    state: &AppState<S>,
    dataset_id: &str,
    selected_pr_urls: &[String],
    model: &str,
    roles: &[String],
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<Review, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    ensure_openrouter_configured()?;

    if selected_pr_urls.is_empty() {
        return Err("Select at least one dataset PR".to_string());
    }

    let dataset_dir = state.config.server.dataset_dir.join(dataset_id);
    let dataset_entries = load_golden_datasets(&dataset_dir)
        .map_err(|error| format!("Failed to load dataset {dataset_id}: {error}"))?;
    let selected_set = selected_pr_urls.iter().cloned().collect::<HashSet<_>>();
    let selected_entries = dataset_entries
        .into_iter()
        .filter(|entry| selected_set.contains(&entry.url))
        .collect::<Vec<_>>();

    if selected_entries.is_empty() {
        return Err("Selected dataset PRs were not found in dataset".to_string());
    }

    let review_id = new_review_id(BENCHMARK_REVIEW_ID_PREFIX);
    let live_agents = resolve_live_agents(roles)?;
    let review = Review {
        id: review_id.clone(),
        agent_sessions: Vec::new(),
        analytics: None,
        duration: None,
        status: ReviewStatus::Running,
        metadata: ReviewMetadata::Plain,
    };

    state
        .store
        .save::<Review>(&review)
        .await
        .map_err(|error| format!("Failed to save running benchmark review: {error}"))?;

    let dashboard_tx = register_live_review(state, &review_id, live_agents).await;
    let task_state = state.clone();
    let task_model = model.to_string();
    let task_roles = roles.to_vec();
    let task_dataset_id = dataset_id.to_string();
    let failure_review = review.clone();
    let task_dashboard_tx = dashboard_tx.clone();
    tokio::spawn(async move {
        run_launch_job_with_recovery(
            task_state.clone(),
            failure_review,
            dashboard_tx,
            "Benchmark",
            run_benchmark_job(
                task_state,
                review_id,
                task_dataset_id,
                selected_entries,
                task_model,
                task_roles,
                reasoning_effort,
                task_dashboard_tx,
            ),
        )
        .await;
    });

    Ok(review)
}

async fn run_review_job<S>(
    state: AppState<S>,
    review_id: MagicTypeId,
    repository: RemoteRepositoryMeta,
    pr_meta: PrMeta,
    diff: String,
    model: String,
    roles: Vec<String>,
    reasoning_effort: Option<ReasoningEffort>,
    dashboard_tx: broadcast::Sender<RunEvent>,
) -> Result<(), String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let started_at = Instant::now();

    let agents = resolve_agents(&roles)?;
    let client = Arc::new(
        openrouter::Client::from_env()
            .map_err(|error| format!("Failed to create OpenRouter client: {error}"))?,
    );
    let cost_tracker = Arc::new(AnalyticsTracker::new());

    let config = EvalConfig {
        review_id: review_id.clone(),
        context: EvalContext {
            repo_root: std::env::temp_dir(),
            ruleset: None,
            repository: repository.clone(),
            pull_request: Some(pr_meta.clone()),
        },
        strategy: EvalStrategy::Panel,
        model: Model(model.clone()),
        reasoning_effort,
        client,
        cache: None,
        cost_tracker: cost_tracker.clone(),
        dashboard_tx: Some(dashboard_tx.clone()),
        agents,
        max_findings: MAX_FINDINGS_PER_AGENT,
        template_vars: None,
    };

    let result = review_diff(Diff::new(diff), &config).await;
    let analytics = cost_tracker.to_snapshot().await;
    let duration = started_at.elapsed();

    match result {
        Ok(findings) => {
            let pr_result = build_pr_result(review_id.clone(), None, Vec::new(), findings);
            state
                .store
                .save::<PrResult>(&pr_result)
                .await
                .map_err(|error| format!("Failed to save PR result: {error}"))?;

            let review = Review {
                id: review_id.clone(),
                agent_sessions: Vec::new(),
                analytics: Some(analytics.clone()),
                duration: Some(duration),
                status: ReviewStatus::Completed,
                metadata: ReviewMetadata::PullRequest(PullRequestReviewMetadata {
                    repository,
                    meta: pr_meta,
                }),
            };
            state
                .store
                .save::<Review>(&review)
                .await
                .map_err(|error| format!("Failed to save completed review: {error}"))?;
            Ok(())
        }
        Err(error) => {
            let review = Review {
                id: review_id.clone(),
                agent_sessions: Vec::new(),
                analytics: Some(analytics.clone()),
                duration: Some(duration),
                status: ReviewStatus::Failed,
                metadata: ReviewMetadata::PullRequest(PullRequestReviewMetadata {
                    repository,
                    meta: pr_meta,
                }),
            };
            state
                .store
                .save::<Review>(&review)
                .await
                .map_err(|save_error| format!("Failed to save failed review: {save_error}"))?;
            emit_terminal_review_event(&dashboard_tx, &review);
            Err(format!("Review pipeline failed: {error}"))
        }
    }
}

async fn run_benchmark_job<S>(
    state: AppState<S>,
    review_id: MagicTypeId,
    dataset_id: String,
    selected_entries: Vec<GoldenCommentEntry>,
    model: String,
    roles: Vec<String>,
    reasoning_effort: Option<ReasoningEffort>,
    dashboard_tx: broadcast::Sender<RunEvent>,
) -> Result<(), String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let dataset_dir = state.config.server.dataset_dir.join(&dataset_id);
    let benchmark_dir = state
        .config
        .server
        .benchmark_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(BENCHMARK_DIR));
    if let Err(error) = fs::create_dir_all(&benchmark_dir) {
        warn!(
            "Failed to create benchmark dir {}: {}",
            benchmark_dir.display(),
            error
        );
    }

    let agents = resolve_agents(&roles)?;
    let client = Arc::new(
        openrouter::Client::from_env()
            .map_err(|error| format!("Failed to create OpenRouter client: {error}"))?,
    );
    let benchmark = run_benchmark(
        review_id.clone(),
        &dataset_dir,
        &benchmark_dir,
        selected_entries,
        Model(model.clone()),
        agents,
        reasoning_effort,
        client,
        Some(dashboard_tx.clone()),
        std::env::temp_dir(),
        MAX_FINDINGS_PER_AGENT,
    )
    .await?;

    for outcome in &benchmark.outcomes {
        state
            .store
            .save::<PrResult>(&outcome.pr_result)
            .await
            .map_err(|error| format!("Failed to save benchmark PR result: {error}"))?;
    }

    let completed_results = benchmark.completed_results();
    let final_status = if completed_results > 0 {
        ReviewStatus::Completed
    } else {
        ReviewStatus::Failed
    };
    let review = Review {
        id: review_id.clone(),
        agent_sessions: Vec::new(),
        analytics: Some(benchmark.analytics.clone()),
        duration: Some(benchmark.duration),
        status: final_status,
        metadata: ReviewMetadata::Plain,
    };
    state
        .store
        .save::<Review>(&review)
        .await
        .map_err(|error| format!("Failed to save benchmark review: {error}"))?;
    emit_terminal_review_event(&dashboard_tx, &review);

    Ok(())
}

async fn run_launch_job_with_recovery<S, F>(
    state: AppState<S>,
    review: Review,
    dashboard_tx: broadcast::Sender<RunEvent>,
    job_kind: &'static str,
    job: F,
) where
    S: Store + Send + Sync + Clone + 'static,
    F: std::future::Future<Output = Result<(), String>> + Send,
{
    let review_id = review.id.clone();
    let result = std::panic::AssertUnwindSafe(job).catch_unwind().await;

    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            error!("{job_kind} launch failed: {error}");
            ensure_terminal_review_state(&state, &review, &dashboard_tx).await;
        }
        Err(payload) => {
            let panic_message = panic_payload_message(payload);
            error!("{job_kind} launch panicked: {panic_message}");
            ensure_terminal_review_state(&state, &review, &dashboard_tx).await;
        }
    }

    unregister_live_review(&state, &review_id).await;
}

async fn ensure_terminal_review_state<S>(
    state: &AppState<S>,
    review: &Review,
    dashboard_tx: &broadcast::Sender<RunEvent>,
) where
    S: Store + Send + Sync + Clone + 'static,
{
    match state.store.load::<Review>(&review.id).await {
        Ok(Some(current_review)) if is_terminal_review_status(&current_review.status) => {}
        Ok(Some(current_review)) => {
            warn!(
                "Review {} left in non-terminal state {} after background job exit; marking failed",
                review.id, current_review.status
            );
            mark_review_failed(state, review, dashboard_tx).await;
        }
        Ok(None) => {
            warn!(
                "Review {} missing after background job exit; writing failed fallback state",
                review.id
            );
            mark_review_failed(state, review, dashboard_tx).await;
        }
        Err(error) => {
            warn!(
                "Failed to reload review {} after background job exit: {}",
                review.id, error
            );
            mark_review_failed(state, review, dashboard_tx).await;
        }
    }
}

fn panic_payload_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else {
        "unknown panic payload".to_string()
    }
}

fn is_terminal_review_status(status: &ReviewStatus) -> bool {
    matches!(
        status,
        ReviewStatus::Completed | ReviewStatus::Failed | ReviewStatus::Cancelled
    )
}

fn new_review_id(prefix: &str) -> MagicTypeId {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{prefix}-{stamp}").create_type_id::<V7>()
}

fn ensure_openrouter_configured() -> Result<(), String> {
    openrouter::Client::from_env()
        .map(|_| ())
        .map_err(|error| format!("Failed to create OpenRouter client: {error}"))
}

async fn register_live_review<S>(
    state: &AppState<S>,
    review_id: &MagicTypeId,
    live_agents: Vec<LiveAgentInfo>,
) -> broadcast::Sender<RunEvent>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let (tx, _rx) = broadcast::channel::<RunEvent>(1024);
    {
        let mut active_reviews = state.active_reviews.write().await;
        if !active_reviews.contains(review_id) {
            active_reviews.push(review_id.clone());
        }
    }
    {
        let mut channels = state.review_channels.write().await;
        channels.insert(review_id.clone(), tx.clone());
    }
    {
        let mut review_agents = state.live_review_agents.write().await;
        review_agents.insert(review_id.clone(), live_agents);
    }
    tx
}

async fn unregister_live_review<S>(state: &AppState<S>, review_id: &MagicTypeId)
where
    S: Store + Send + Sync + Clone + 'static,
{
    {
        let mut active_reviews = state.active_reviews.write().await;
        active_reviews.retain(|active_id| active_id != review_id);
    }
    {
        let mut channels = state.review_channels.write().await;
        channels.remove(review_id);
    }
    {
        let mut review_agents = state.live_review_agents.write().await;
        review_agents.remove(review_id);
    }
}

fn terminal_review_event(review: &Review) -> RunEvent {
    let analytics = review.analytics.clone().unwrap_or_default();
    match review.status {
        ReviewStatus::Completed => RunEvent::ReviewCompleted {
            review_id: review.id.clone(),
            analytics,
        },
        _ => RunEvent::ReviewFailed {
            review_id: review.id.clone(),
            analytics,
        },
    }
}

fn emit_terminal_review_event(dashboard_tx: &broadcast::Sender<RunEvent>, review: &Review) {
    if let Err(error) = dashboard_tx.send(terminal_review_event(review)) {
        warn!(
            "Failed to emit terminal event for review {}: {}",
            review.id, error
        );
    }
}

async fn mark_review_failed<S>(
    state: &AppState<S>,
    review: &Review,
    dashboard_tx: &broadcast::Sender<RunEvent>,
) where
    S: Store + Send + Sync + Clone + 'static,
{
    let mut failed_review = review.clone();
    failed_review.status = ReviewStatus::Failed;

    if let Err(error) = state.store.save::<Review>(&failed_review).await {
        error!(
            "Failed to save failed review state for {}: {}",
            failed_review.id, error
        );
    }

    emit_terminal_review_event(dashboard_tx, &failed_review);
}

async fn reconcile_review_state<S>(state: &AppState<S>, review: &mut Review)
where
    S: Store + Send + Sync + Clone + 'static,
{
    if !matches!(review.status, ReviewStatus::Pending | ReviewStatus::Running) {
        return;
    }

    let is_active = {
        let active_reviews = state.active_reviews.read().await;
        active_reviews.contains(&review.id)
    };

    if !is_active {
        review.status = ReviewStatus::Failed;
        if let Err(error) = state.store.save::<Review>(review).await {
            warn!(
                "Failed to reconcile stale review {} to failed state: {}",
                review.id, error
            );
        }
    }
}

fn resolve_agents(roles: &[String]) -> Result<&'static [&'static AgentEntry], String> {
    Ok(Box::leak(resolve_agent_entries(roles)?.into_boxed_slice()))
}

fn resolve_live_agents(roles: &[String]) -> Result<Vec<LiveAgentInfo>, String> {
    Ok(resolve_agent_entries(roles)?
        .into_iter()
        .map(|agent| LiveAgentInfo {
            id: agent.agent_id.clone(),
            name: agent.role_name.clone(),
            abbreviation: agent.role_abbreviation.clone(),
        })
        .collect())
}

fn resolve_agent_entries(roles: &[String]) -> Result<Vec<&'static AgentEntry>, String> {
    let library = PromptLibrary::new().map_err(|error| error.to_string())?;

    let resolved = if roles.is_empty() {
        let mut abbreviations = library
            .abbreviations()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        abbreviations.sort();
        abbreviations
            .into_iter()
            .filter_map(|abbreviation| library.config(&abbreviation))
            .collect::<Vec<_>>()
    } else {
        let mut agents = Vec::with_capacity(roles.len());
        let mut missing = Vec::new();
        for role in roles {
            match library.config(role) {
                Some(agent) => agents.push(agent),
                None => missing.push(role.clone()),
            }
        }
        if !missing.is_empty() {
            return Err(format!("Unknown agent roles: {}", missing.join(", ")));
        }
        agents
    };

    if resolved.is_empty() {
        return Err("No agents resolved from PromptLibrary".to_string());
    }

    Ok(resolved)
}

fn result_matches_review(result: &PrResult, review_id: &MagicTypeId, review_key: &str) -> bool {
    result.id == *review_id
        || result.benchmark_id.as_ref() == Some(review_id)
        || result.id.to_string().starts_with(review_key)
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

fn build_review_agent_log(review_id: &MagicTypeId, session: &AgentSession) -> ReviewAgentLog {
    let messages = ordered_messages(session);

    let prompt = messages
        .iter()
        .filter_map(|message| match message {
            RoleMessage::System(text) | RoleMessage::User(text) => Some(text.clone()),
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let response = messages
        .iter()
        .filter_map(|message| match message {
            RoleMessage::Assistant(response) => Some(response.output.clone()),
            RoleMessage::Tool(invocation) => Some(format_tool_message(invocation)),
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let reasoning = messages
        .iter()
        .filter_map(|message| match message {
            RoleMessage::Assistant(response) => Some(response.thinking.clone()),
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    ReviewAgentLog {
        review_id: review_id.clone(),
        agent_id: session.id.clone(),
        model_name: session.model_name.clone(),
        prompt,
        response,
        reasoning,
    }
}

fn ordered_messages(session: &AgentSession) -> Vec<RoleMessage> {
    let mut turns: Vec<AgentTurn> = session.turns.clone();
    turns.sort_by_key(|turn| turn.turn_index);

    turns
        .into_iter()
        .flat_map(|turn| {
            let mut messages: Vec<AgentTurnMessage> = turn.messages;
            messages.sort_by_key(|message| message.msg_index);
            messages.into_iter().map(RoleMessage::from)
        })
        .collect()
}

fn format_tool_message(invocation: &ToolInvocation) -> String {
    let input = serde_json::to_string_pretty(&invocation.input)
        .unwrap_or_else(|_| invocation.input.to_string());
    let output = serde_json::to_string_pretty(&invocation.output)
        .unwrap_or_else(|_| invocation.output.to_string());

    let mut sections = vec![format!("[tool] {}", invocation.tool_name)];
    if !input.trim().is_empty() && input != "null" {
        sections.push(format!("input:\n{input}"));
    }
    if !output.trim().is_empty() && output != "null" {
        sections.push(format!("output:\n{output}"));
    }

    sections.join("\n")
}

fn extract_pr_number(url: &str) -> u32 {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|segment| segment.parse::<u32>().ok())
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use riv_types::agent::AgentResponse;

    #[test]
    fn test_extract_pr_number() {
        assert_eq!(extract_pr_number("https://github.com/a/b/pull/42"), 42);
        assert_eq!(extract_pr_number("not-a-pr"), u32::MAX);
    }

    #[test]
    fn test_build_review_agent_log() {
        let review_id = MagicTypeId::default();
        let session_id = MagicTypeId::default();
        let session = AgentSession {
            id: session_id.clone(),
            review_id: Some(review_id.clone()),
            model_name: "openai/gpt-5-mini".to_string(),
            turns: vec![AgentTurn {
                id: None,
                session_id,
                turn_index: 0,
                messages: vec![
                    RoleMessage::System("System prompt".to_string()).into(),
                    RoleMessage::User("Review this diff".to_string()).into(),
                    RoleMessage::Assistant(AgentResponse {
                        thinking: "Need inspect auth flow".to_string(),
                        output: "Found potential bug".to_string(),
                    })
                    .into(),
                    AgentTurnMessage {
                        id: None,
                        turn_id: 0,
                        msg_index: 3,
                        role: "tool".to_string(),
                        text_content: None,
                        thinking: None,
                        output: None,
                        tool_name: Some("grep".to_string()),
                        tool_input: Some("{\"regex\":\"auth\"}".to_string()),
                        tool_output: Some("[\"src/auth.rs\"]".to_string()),
                    },
                ],
            }],
        };

        let log = build_review_agent_log(&review_id, &session);
        assert!(log.prompt.contains("System prompt"));
        assert!(log.prompt.contains("Review this diff"));
        assert!(log.response.contains("Found potential bug"));
        assert!(log.response.contains("[tool] grep"));
        assert!(log.reasoning.contains("Need inspect auth flow"));
    }
}
