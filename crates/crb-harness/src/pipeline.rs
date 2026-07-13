use std::sync::Arc;

use anyhow::Result;
use crb_consensus::{CacheBackend, adaptive::get_agents_for_diff};
use crb_shared::{
    diff::{self, Diff},
    finding::Finding,
};
use crb_tools::{build_tool_server, create_linter_tool, linters::tool::LinterArgs};
use crb_types::RunEvent;
use rig_core::tool::Tool;
use tokio::task;
use tracing::{info, warn};

use crate::{eval::EvalConfig, finding::post_process_findings};

// Just a helper macro to send events to the dashboard if the channel is available
#[macro_export]
macro_rules! send_event {
    ($event:expr) => {
        if let Some(tx) = config.dashboard_tx {
            let _ = tx.send($event);
        }
    };
}

// TODO: Metrics, Reporting
pub async fn evaluate(diff: Diff, config: &EvalConfig) -> Result<()> {
    send_event!(RunEvent::ReviewStarted {
        identifier: config.identifier.id().to_string(),
        total_agents: config.agents.len(), // TODO: This is not accurate, as some agents may not be applicable to the diff
    });

    diff::preprocess_diff(&mut diff);

    let linters = run_linters(config);
    let reviewers = run_reviewers(&diff, config);
    let (linter_findings, reviewer_findings) = tokio::join!(linters, reviewers);

    let reviewer_findings = post_process(reviewer_findings.as_slice(), config);

    metrics();
    report();

    todo!()
}

async fn run_linters(config: &EvalConfig) -> Vec<Finding> {
    let mut linter_findings: Vec<Finding> = Vec::new();
    if let Some(ref configs) = config.linter_configs {
        let mut linter_set = tokio::task::JoinSet::new();
        for (_, lconfig) in configs {
            let tool = create_linter_tool(lconfig);
            let args = LinterArgs {
                repo_path: config.repo_root.to_string_lossy().to_string(),
            };
            linter_set.spawn(async move {
                let result = tool.call(args).await;
                result
            });
        }

        while let Some(res) = linter_set.join_next().await {
            match res {
                Ok(Ok(findings)) => linter_findings.extend(findings),
                Ok(Err(e)) => warn!("Linter failed: {e}"),
                Err(e) => warn!("Linter join error: {e}"),
            }
        }

        info!(
            "Found {} linter finding(s) for repo {:?}",
            linter_findings.len(),
            config.repo_root
        );
    }

    todo!()
}

// TODO: Rules
async fn run_reviewers(diff: &Diff, config: &EvalConfig) -> Vec<Finding> {
    let collector = None;
    let tool_server = build_tool_server(config.repo_root.to_str(), collector).run();
    let effective_agents = get_agents_for_diff(diff, config.agents);

    let agent_set = task::JoinSet::new();
    for agent in effective_agents {
        let tool_server = tool_server.clone();
        let diff = diff.clone();
        let config = config.clone();
        agent_set.spawn(async move {
            let findings = agent.review(&diff, &tool_server, &config).await;
            findings
        });
    }

    {
        let mut reviewer_set = tokio::task::JoinSet::new();
    }
}

fn post_process(findings: &[Finding], config: &EvalConfig) -> Vec<Finding> {
    post_process_findings(findings)
}

fn metrics() {}

fn report() {}
