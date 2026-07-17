use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crb_agents::agent::AgentEntry;
use crb_cache::traits::CacheBackend;
use crb_reporting::cost::AnalyticsTracker;
use crb_rules::RuleSet;
use crb_types::{
    RunEvent,
    capabilities::ReasoningEffort,
    vcs::{pr::PrMeta, repository::RepositoryMeta},
    wrappers::{Model, WrappedData},
};
use rig_core::{agent::Agent, providers::openai, tool::server::ToolServerHandle};

/// Strategy for evaluating a PR review.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalStrategy {
    /// Run a single-agent evaluation with a single generalised expert.
    Single,

    /// Run a full multi-agent evaluation with domain experts.
    Panel,
}

pub trait EvalIdentifier: Send + Sync {
    fn id(&self) -> &str;
}

/// Configuration for an evaluation run.
#[derive(Clone)]
pub struct EvalConfig {
    /// Unique identifier for the run.
    pub identifier: String,

    pub context: EvalContext,

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

    /// broadcast channel for sending run events to a dashboard.
    pub dashboard_tx: Option<tokio::sync::broadcast::Sender<RunEvent>>,

    /// The available agents for the evaluation.
    ///
    /// An agent being available does not mean it will be used in the evaluation.
    /// The agents used will depend on the strategy and the roles defined.
    pub agents: &'static [&'static AgentEntry],

    /// The repository root path for the evaluation.
    pub repo_root: PathBuf,

    /// Maximum number of findings per agent.
    pub max_findings: usize,

    /// The model to use for the judge.
    pub judge_model: Model,

    /// Judge agent for evaluating findings against goldens.
    pub judge: Agent<openai::responses_api::ResponsesCompletionModel>,

    /// Ruleset for formatting additional context.
    pub ruleset: Option<Arc<RuleSet>>,

    /// Template variables for the agent prompts.
    pub template_vars: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Clone)]
pub struct EvalContext {
    repository: RepositoryMeta,
    pull_request: Option<PrMeta>,
}

impl std::fmt::Debug for EvalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvalConfig")
            .field("strategy", &self.strategy)
            .field("model", &self.model.get())
            .field("reasoning_effort", &self.reasoning_effort)
            .field("agents", &self.agents)
            .field("max_findings", &self.max_findings)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_strategy_partial_eq() {
        assert_eq!(EvalStrategy::Single, EvalStrategy::Single);
        assert_eq!(EvalStrategy::Panel, EvalStrategy::Panel);
        assert_ne!(EvalStrategy::Single, EvalStrategy::Panel);
    }

    #[test]
    fn test_eval_strategy_debug() {
        assert_eq!(format!("{:?}", EvalStrategy::Single), "Single");
        assert_eq!(format!("{:?}", EvalStrategy::Panel), "Panel");
    }

    #[test]
    fn test_eval_strategy_clone() {
        let original = EvalStrategy::Panel;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_eval_identifier_impl() {
        struct TestId(String);

        impl EvalIdentifier for TestId {
            fn id(&self) -> &str {
                &self.0
            }
        }

        let id = TestId("test-runner".to_string());
        assert_eq!(id.id(), "test-runner");
    }
}
