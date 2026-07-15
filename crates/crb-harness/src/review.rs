use anyhow::{Context, Result};
use crb_shared::{diff::Diff, finding::Finding};
use crb_types::wrappers::WrappedData;
use tracing::info;

use crate::eval::{EvalConfig, EvalIdentifier};
use crate::pipeline;

/// Simple identifier for review CLI runs.
struct ReviewIdentifier(String);

impl EvalIdentifier for ReviewIdentifier {
    fn id(&self) -> &str {
        &self.0
    }
}

/// Review a PR diff.
pub async fn review_pr(diff: Diff, config: &EvalConfig) -> Result<Vec<Finding>> {
    info!(
        "Reviewing diff ({} bytes, {} sections) with {} agents, model={}",
        diff.raw.len(),
        diff.sections.len(),
        config.agents.len(),
        config.model.get(),
    );

    let findings = pipeline::evaluate(diff, config)
        .await
        .context("Pipeline evaluation failed")?;

    info!("Review complete: {} findings", findings.len());
    Ok(findings)
}

/// Build an `EvalConfig` from `ReviewArgs` for a one-shot review.
#[cfg(feature = "binary")]
pub fn build_review_config(args: &crate::config::ReviewArgs) -> Result<EvalConfig> {
    use crb_agents::AgentEntry;
    use crb_reporting::cost::AnalyticsTracker;
    use crb_types::wrappers::Model;
    use rig_core::agent::Agent;
    use rig_core::client::{CompletionClient, ProviderClient};
    use rig_core::providers::openai;
    use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
    use rig_core::tool::server::ToolServer;

    let client = openai::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;
    let client = Arc::new(client);

    let tool_server = ToolServer::new().run();

    // Resolve typed agent entries through PromptLibrary
    let agents: Vec<&'static AgentEntry> = match &args.roles {
        Some(abbrevs) => {
            let lib = crb_agents::prompts::PromptLibrary::get_instance();
            abbrevs
                .iter()
                .filter_map(|a| lib.config(a.trim()))
                .collect()
        }
        None => crb_agents::prompts::PromptLibrary::get_instance()
            .agents()
            .into_iter()
            .collect(),
    };
    // Leak to get 'static lifetime required by EvalConfig.agents
    let agents: &'static [&'static AgentEntry] = Box::leak(agents.into_boxed_slice());

    let cost_tracker = Arc::new(AnalyticsTracker::new());

    // Build a minimal judge agent (required by EvalConfig, unused by review pipeline::evaluate)
    let judge: Agent<ResponsesCompletionModel> = client
        .as_ref()
        .agent(&args.model)
        .preamble("You are a code review judge.")
        .temperature(0.3)
        .build();

    let model = Model(args.model.clone());

    Ok(EvalConfig {
        strategy: crate::eval::EvalStrategy::Panel,
        identifier: "review-cli".to_string(),
        model,
        reasoning_effort: None,
        client,
        cache: None,
        cost_tracker,
        tool_handle: tool_server,
        dashboard_tx: None,
        agents,
        repo_root: args.path.clone(),
        max_findings: args.max_findings,
        judge_model: args.model.clone(),
        judge,
        linters_only: false,
        linter_configs: None,
        ruleset: None,
        template_vars: None,
    })
}
