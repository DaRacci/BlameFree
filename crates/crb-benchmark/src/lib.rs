//! Benchmark execution for code review evaluation.

use core::error;
use std::{fmt, sync::Arc};

use crb_agents::{
    AgentConfig, AgentConfigProvider, AgentDetailsProvider, RuntimeProvider, build_agent,
    stream_agent,
};
use crb_cache::traits::CacheBackend;
use crb_reporting::cost::AnalyticsTracker;
use crb_types::{
    RunEvent,
    benchmark::{
        golden::GoldenCommentEntry,
        judge::{JudgeVerdict, JudgedFindings},
    },
    capabilities::ReasoningEffort,
    errors::ManyErrors,
    finding::Finding,
    wrappers::Model,
};
use mti::prelude::{MagicTypeId, MagicTypeIdExt, V7};
use rig_core::{
    agent::Agent,
    client::Client,
    completion::CompletionModel,
    providers::openrouter::{self, OpenRouterExt},
    tool::server::ToolServer,
};
use tokio::{
    sync::{broadcast::Sender, mpsc::Sender},
    task::JoinSet,
};

use crate::judge::judge_finding;

pub mod diffs;
pub mod judge;
pub mod pr;
pub mod scaffold;

pub const BENCHMARK_DIR: &str = "benchmark";
pub const BENCHMARK_DIFFS_SUBDIR: &str = "diffs";
pub const BENCHMARK_WORKTREE_SUBDIR: &str = "worktree";
pub const BENCHMARK_BASE_REPOS_SUBDIR: &str = "base_repos";

pub const DATASETS_DIR: &str = "datasets/golden_comments";

pub struct BenchmarkConfig {
    /// Unique identifier for the benchmark
    pub id: MagicTypeId,

    /// Model to use for the judge reviewer
    pub model: Model,

    /// The reasoning level to use for the judge reviewer.
    pub reasoning_effort: Option<ReasoningEffort>,

    /// Shared client for interacting with the OpenRouter API.
    pub client: Arc<openrouter::Client>,

    /// Cache backend for storing and retrieving benchmark data.
    pub cache: Arc<dyn CacheBackend>,

    /// Analytics tracker for monitoring benchmark execution and performance.
    pub analytics: Arc<AnalyticsTracker>,

    /// Broadcast channel sender for sending run events to the dashboard.
    pub dashboard_tx: Option<Sender<RunEvent>>,
}

impl AgentConfigProvider for BenchmarkConfig {
    fn get_agent_config(&self) -> crb_agents::AgentConfig<'_> {
        AgentConfig {
            model: &self.model,
            client: &self.client,
            template_vars: None,
            additional_params: None,
        }
    }
}

impl AgentDetailsProvider for BenchmarkConfig {
    fn get_name(&self) -> &str {
        "Benchmark Judge"
    }
    fn get_description(&self) -> &str {
        "Benchmark execution for code review evaluation."
    }

    // There is no system prompt for the benchmark judge, so we return an empty string.
    // The entire prompt is sent as the user message.
    fn get_prompt(&self, _: std::collections::HashMap<String, serde_json::Value>) -> String {
        String::new()
    }
}

impl<A> RuntimeProvider<A> for BenchmarkConfig
where
    A: CompletionModel + Send + Sync + 'static,
{
    fn get_id(&self) -> &MagicTypeId {
        &self.id
    }

    fn get_analytics(&self) -> Arc<AnalyticsTracker> {
        self.analytics.clone()
    }

    fn get_dashboard_tx(&self) -> Option<Sender<RunEvent>> {
        self.dashboard_tx.clone()
    }

    fn get_client(&self) -> Arc<Client<OpenRouterExt<A>>> {
        self.client.clone()
    }
}

impl fmt::Debug for BenchmarkConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BenchmarkConfig")
            .field("id", &self.id)
            .field("model", &self.model)
            .field("reasoning_effort", &self.reasoning_effort)
            .finish()
    }
}

pub async fn evaluate_findings(
    config: Arc<BenchmarkConfig>,
    dataset: &GoldenCommentEntry,
    findings: &[Finding],
) -> (JudgedFindings, Option<ManyErrors>) {
    let tools = ToolServer::new().run();
    let agent = build_agent(config.clone(), &*config, tools)
        .output_schema::<JudgeVerdict>()
        .build();

    let judge_set = JoinSet::new();
    let mut verdicts = Vec::new();
    let mut errors = ManyErrors::new();
    for finding in findings {
        let (finding_verdicts, finding_errors) =
            judge_finding(config.clone(), finding, golden_comments).await;

        verdicts.extend(finding_verdicts);
        if let Some(finding_errors) = finding_errors {
            warn!(
                "Errors occurred while judging finding {}: {:?}",
                finding.id, finding_errors
            );
            errors.extend(finding_errors);
        }
    }

    let mut judged_findings = JudgedFindings::default();
    for verdict in verdicts {
        let Some(finding) = findings.iter().find(|f| f.id == verdict.finding_id) else {
            error!(
                "How did this happen? Verdict for finding_id {} but no such finding exists in the provided findings.",
                verdict.finding_id
            );
            continue;
        };

        let mut f = finding.clone();
        f.verdict = Some(verdict);
        judged_findings.findings.push(f);
    }

    for comments in &dataset.comments {}

    todo!()
}
