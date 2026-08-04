//! Benchmark execution for code review evaluation.

use std::{fmt, sync::Arc};

use mti::prelude::MagicTypeId;
use riv_agents::{AgentConfig, AgentConfigProvider, AgentDetailsProvider};
use riv_cache::traits::CacheBackend;
use riv_reporting::cost::AnalyticsTracker;
use riv_types::{
    RunEvent,
    benchmark::{golden::GoldenCommentEntry, judge::JudgedFindings},
    capabilities::ReasoningEffort,
    errors::ManyErrors,
    finding::Finding,
    wrappers::Model,
};
use tokio::sync::broadcast::Sender;

use rig_core::providers::openrouter;

pub mod diffs;
pub mod judge;
pub mod pr;
mod run;
pub mod scaffold;

pub use run::{BenchmarkPrOutcome, BenchmarkRunResult, run_benchmark};

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

impl riv_agents::RuntimeProvider for BenchmarkConfig {
    fn get_id(&self) -> &MagicTypeId {
        &self.id
    }

    fn get_analytics(&self) -> Arc<AnalyticsTracker> {
        self.analytics.clone()
    }

    fn get_dashboard_tx(&self) -> Option<Sender<RunEvent>> {
        self.dashboard_tx.clone()
    }
}

impl AgentConfigProvider for BenchmarkConfig {
    fn get_agent_config(&self) -> riv_agents::AgentConfig<'_> {
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
    _config: Arc<BenchmarkConfig>,
    dataset: &GoldenCommentEntry,
    findings: &[Finding],
) -> (JudgedFindings, Option<ManyErrors>) {
    let _ = dataset;
    let _ = findings;

    todo!()
}
