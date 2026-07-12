use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crb_consensus::CacheBackend;
use crb_rules::RuleSet;
use crb_tools::linters::config::LinterConfig;
use crb_types::RunEvent;
use rig_core::{agent::Agent, providers::openai};

use crate::{cost, model_capabilities::ReasoningEffort};

/// Strategy for evaluating a PR review.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalStrategy {
    /// Run a single-agent evaluation with a single generalised expert.
    SingleAgent,

    /// Run a full multi-agent evalutation with domain experts and a consensus judge.
    Consensus,
}

/// Configuration for an evaluation run.
#[derive(Clone)]
pub struct EvalConfig {
    // Strategy selection
    pub strategy: EvalStrategy,

    // Model/LLM config
    pub model: String,
    pub judge_model: String,
    pub reasoning_effort: Option<ReasoningEffort>,

    // Shared services
    pub client: Arc<openai::Client>,
    pub judge: Agent<openai::responses_api::ResponsesCompletionModel>,
    pub cache: Option<Arc<dyn CacheBackend>>,
    pub cost_tracker: Arc<cost::CostTracker>,
    pub dashboard_tx: Option<tokio::sync::broadcast::Sender<RunEvent>>,

    // Evaluation parameters
    pub roles: String,
    pub max_findings: usize,
    pub linters_only: bool,
    pub linter_configs: Option<HashMap<String, LinterConfig>>,
    pub ruleset: Option<RuleSet>,
    pub cache_dir: Option<PathBuf>,
    pub benchmark_dir: Option<PathBuf>,

    // Consensus-specific
    pub workdir: Option<String>,

    // Other options
    pub template_vars: Option<HashMap<String, serde_json::Value>>,
}

impl std::fmt::Debug for EvalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvalConfig")
            .field("strategy", &self.strategy)
            .field("model", &self.model)
            .field("judge_model", &self.judge_model)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("roles", &self.roles)
            .field("max_findings", &self.max_findings)
            .field("linters_only", &self.linters_only)
            .field("linter_configs", &self.linter_configs)
            .field("ruleset", &self.ruleset)
            .field("cache_dir", &self.cache_dir)
            .field("benchmark_dir", &self.benchmark_dir)
            .field("workdir", &self.workdir)
            .field("template_vars", &self.template_vars)
            .finish_non_exhaustive()
    }
}
