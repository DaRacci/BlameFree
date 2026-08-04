use anyhow::{Context, Result};
use riv_shared::diff::Diff;
use riv_types::finding::Finding;
use riv_types::wrappers::WrappedData;
use tracing::info;

use crate::eval::EvalConfig;
use crate::pipeline;

/// Review a diff.
pub async fn review_diff(diff: Diff, config: &EvalConfig) -> Result<Vec<Finding>> {
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
    use std::sync::Arc;

    use riv_agents::AgentEntry;
    use riv_reporting::cost::AnalyticsTracker;
    use riv_types::vcs::repository::{RemoteRepositoryMeta, VCSPlatform};
    use riv_types::wrappers::Model;

    let client = Arc::new(riv_shared::build_client()?);

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
    let review_id = crate::cli_review_id();

    Ok(EvalConfig {
        review_id,
        context: crate::eval::EvalContext {
            repo_root: args.path.clone(),
            ruleset: None,
            // A plain-diff CLI review has no hosting repository metadata.
            repository: RemoteRepositoryMeta {
                owner: String::new(),
                name: String::new(),
                platform: VCSPlatform::GitHub,
            },
            pull_request: None,
        },
        strategy: crate::eval::EvalStrategy::Panel,
        model,
        reasoning_effort: None,
        client,
        cache: None,
        cost_tracker,
        dashboard_tx: None,
        agents,
        max_findings: args.max_findings,
        template_vars: None,
    })
}
