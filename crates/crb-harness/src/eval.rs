use std::{collections::HashMap, fmt, path::PathBuf, sync::Arc};

use crb_agents::{AgentConfig, AgentConfigProvider, agent::AgentEntry};
use crb_cache::traits::CacheBackend;
use crb_reporting::cost::AnalyticsTracker;
use crb_rules::RuleSet;
use crb_types::{
    RunEvent,
    capabilities::ReasoningEffort,
    vcs::{pr::PrMeta, repository::RemoteRepositoryMeta},
    wrappers::{Model, WrappedData},
};
use mti::prelude::MagicTypeId;
use rig_core::providers::openrouter;

/// Strategy for evaluating a PR review.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalStrategy {
    /// Run a single-agent evaluation with a single generalised expert.
    Single,

    /// Run a full multi-agent evaluation with domain experts.
    Panel,
}

/// Configuration for an evaluation run.
#[derive(Clone)]
pub struct EvalConfig {
    /// Unique identifier for the review.
    pub review_id: MagicTypeId,

    pub context: EvalContext,

    /// The strategy to use.
    pub strategy: EvalStrategy,

    /// The model to use for each agent.
    pub model: Model,

    /// The reasoning level to use for each agent.
    pub reasoning_effort: Option<ReasoningEffort>,

    /// A Shared API client for spawning requests to the LLM provider.
    pub client: Arc<openrouter::Client>,

    /// Cache backend for storing and retrieving results, along with all responses from the LLM provider.
    pub cache: Option<Arc<dyn CacheBackend>>,

    /// Cost tracker for use during the run.
    pub cost_tracker: Arc<AnalyticsTracker>,

    /// broadcast channel for sending run events to a dashboard.
    pub dashboard_tx: Option<tokio::sync::broadcast::Sender<RunEvent>>,

    /// The available agents for the evaluation.
    ///
    /// An agent being available does not mean it will be used in the evaluation.
    /// The agents used will depend on the strategy and the roles defined.
    pub agents: &'static [&'static AgentEntry],

    /// The repository root path for the evaluation.
    #[deprecated = "Use `context.repo_root` instead."]
    pub repo_root: PathBuf,

    /// Maximum number of findings per agent.
    pub max_findings: usize,

    /// Ruleset for formatting additional context.
    #[deprecated = "Use `context.ruleset` instead."]
    pub ruleset: Option<Arc<RuleSet>>,

    /// Template variables for the agent prompts.
    pub template_vars: Option<HashMap<String, serde_json::Value>>,
}

impl AgentConfigProvider for EvalConfig {
    fn get_agent_config(&self) -> AgentConfig<'_> {
        AgentConfig {
            model: &self.model,
            additional_params: None,
            client: &self.client,
            template_vars: self.template_vars.as_ref(),
        }
    }
}

#[derive(Clone)]
pub struct EvalContext {
    pub repo_root: PathBuf,
    pub ruleset: Option<Arc<RuleSet>>,
    pub repository: RemoteRepositoryMeta,
    pub pull_request: Option<PrMeta>,
}

impl fmt::Debug for EvalConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalConfig")
            .field("strategy", &self.strategy)
            .field("model", &self.model.get())
            .field("reasoning_effort", &self.reasoning_effort)
            .field("agents", &self.agents)
            .field("max_findings", &self.max_findings)
            .finish_non_exhaustive()
    }
}
