use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use mti::prelude::MagicTypeId;
use rig_core::agent::{Agent, MultiTurnStreamItem, PromptHook};
use rig_core::completion::{CompletionModel, GetTokenUsage};
use rig_core::message::{AssistantContent, ToolResultContent};
use rig_core::streaming::{
    StreamedAssistantContent, StreamedUserContent, StreamingPrompt, ToolCallDeltaContent,
};
use riv_agents::send_event;
use riv_reporting::cost::SessionUsageProvider;
use riv_shared::diff::Diff;
use riv_types::RunEvent;
use riv_types::agent::{AgentChunk, ToolByte};
use riv_types::cost::SessionUsage;
use riv_types::finding::Finding;
use riv_types::wrappers::WrappedData;
use serde::de::DeserializeOwned;
use tracing::{error, info};

use crate::eval::EvalConfig;
use crate::pipeline;

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
    use rig_core::agent::Agent;
    use rig_core::client::{CompletionClient, ProviderClient};
    use rig_core::providers::openrouter;
    use rig_core::providers::openrouter::responses_api::ResponsesCompletionModel;
    use rig_core::tool::server::ToolServer;
    use riv_agents::AgentEntry;
    use riv_reporting::cost::AnalyticsTracker;
    use riv_types::wrappers::Model;

    let client = Arc::new(client);

    let tool_server = ToolServer::new().run();

    let agents: Vec<&'static AgentEntry> = match &args.roles {
        Some(abbrevs) => {
            let lib = riv_agents::prompts::PromptLibrary::get_instance();
            abbrevs
                .iter()
                .filter_map(|a| lib.config(a.trim()))
                .collect()
        }
        None => riv_agents::prompts::PromptLibrary::get_instance()
            .agents()
            .into_iter()
            .collect(),
    };
    // Leak to get 'static lifetime required by EvalConfig.agents
    let agents: &'static [&'static AgentEntry] = Box::leak(agents.into_boxed_slice());

    let cost_tracker = Arc::new(AnalyticsTracker::new());

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
        linter_configs: None,
        ruleset: None,
        template_vars: None,
    })
}
