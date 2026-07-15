use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crb_agents::agent::AgentEntry;
use crb_cache::traits::CacheBackend;
use crb_reporting::cost::AnalyticsTracker;
use crb_rules::RuleSet;
use crb_tools::linters::config::LinterConfig;
use crb_types::{
    RunEvent,
    wrappers::{Model, WrappedData},
};
use rig_core::{agent::Agent, providers::openai, tool::server::ToolServerHandle};

use crate::{cost, model_capabilities::ReasoningEffort};

/// Strategy for evaluating a PR review.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalStrategy {
    /// Run a single-agent evaluation with a single generalised expert.
    Single,

    /// Run a full multi-agent evaluation with domain experts.
    Panel,
}

pub trait EvalIdentifier: Send + Sync + Sized {
    fn id(&self) -> &str;
}

/// Configuration for an evaluation run.
#[derive(Clone)]
pub struct EvalConfig {
    /// Unique identifier for the run.
    pub identifier: Box<dyn EvalIdentifier>,

    /// The strategy to use.
    pub strategy: EvalStrategy,

    /// The model to use for each agent.
    pub model: Model,

    /// The reasoning level to use for each agent.
    pub reasoning_effort: Option<ReasoningEffort>,

    /// A Shared API client for spawning requests to the LLM provider.
    pub client: Arc<openai::Client>,

    /// Cache backend for storing and retrieving results, along with all responses from the LLM provider.
    pub cache: Option<Arc<dyn CacheBackend>>,

    /// Cost tracker for use during the run.
    pub cost_tracker: Arc<AnalyticsTracker>,

    /// A shared tool handle for shared use across agents.
    pub tool_handle: ToolServerHandle,

    /// Optional broadcast channel for sending run events to a dashboard.
    pub dashboard_tx: Option<tokio::sync::broadcast::Sender<RunEvent>>,

    /// The available agents for the evaluation.
    ///
    /// An agent being available does not mean it will be used in the evaluation.
    /// The agents used will depend on the strategy and the roles defined.
    pub agents: &'static [&'static AgentEntry],

    /// The repository root path for the evaluation.
    pub repo_root: PathBuf,

    /// Comma-separated role abbreviations for the evaluation.
    pub roles: String,

    /// Maximum number of findings per agent.
    pub max_findings: usize,

    /// The model to use for the judge.
    pub judge_model: String,

    /// Judge agent for evaluating findings against goldens.
    pub judge: Agent<openai::responses_api::ResponsesCompletionModel>,

    /// Only run linters, skip LLM agents.
    pub linters_only: bool,

    /// Linter configurations.
    pub linter_configs: Option<Arc<HashMap<String, LinterConfig>>>,

    /// Ruleset for formatting additional context.
    pub ruleset: Option<Arc<RuleSet>>,

    /// Template variables for the agent prompts.
    pub template_vars: Option<HashMap<String, serde_json::Value>>,
}

impl std::fmt::Debug for EvalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvalConfig")
            .field("strategy", &self.strategy)
            .field("model", &self.model.get())
            .field("reasoning_effort", &self.reasoning_effort)
            .field("agents", &self.agents)
            .field("roles", &self.roles)
            .field("max_findings", &self.max_findings)
            .finish_non_exhaustive()
    }
}
