use std::sync::Arc;

use anyhow::Result;
use crb_agents::build_agent;
use crb_consensus::adaptive::get_agents_for_diff;
use crb_shared::diff::{self, Diff};
use crb_tools::build_tool_server;

use crb_types::errors::ManyErrors;
use crb_types::{RunEvent, finding::Finding};

use tokio::task;
use tracing::{error, info, warn};

use crate::finding::post_process_findings;
use crate::{eval::EvalConfig, review::stream_agent};

// Helper macro to send events to the dashboard if the channel is available.
// `$config` must be an expression that yields an `&EvalConfig`.
#[macro_export]
macro_rules! send_event {
    ($config:expr, $event:expr) => {
        if let Some(tx) = &$config.dashboard_tx {
            let _ = tx.send($event);
        }
    };
}

/// Send AgentStarted events for each configured agent.
// pub async fn send_agent_started_events(config: &EvalConfig, identifier: &str) {
//     if let Some(ref tx) = config.dashboard_tx {
//         let pr_key = sanitize_filename(identifier);
//         for entry in config.agents {
//             let _ = tx.send(RunEvent::AgentStarted {
//                 identifier: pr_key.clone(),
//                 agent: entry.role_abbreviation.to_string(),
//             });
//         }
//     }
// }

pub async fn evaluate(mut diff: Diff, config: &EvalConfig) -> Result<Vec<Finding>> {
    send_event!(
        config,
        RunEvent::ReviewStarted {
            review_id: config.review_id.clone(),
            agent_ids: config.agents.iter().map(|a| a.agent_id.clone()).collect(),
        }
    );

    diff::preprocess_diff(&mut diff);

    let linters = run_linters(config);
    let reviewers = run_reviewers(&diff, config);
    let (mut all_findings, (reviewer_findings, errors)) = tokio::join!(linters, reviewers);

    let reviewer_findings = post_process(reviewer_findings.as_slice(), config);
    all_findings.extend(reviewer_findings);

    metrics(config).await;
    report(config).await;

    if let Some(errors) = errors {
        error!(
            "Encountered {} error(s) during review\n{}",
            errors.len(),
            errors
        );
    }

    send_event!(
        config,
        RunEvent::ReviewCompleted {
            review_id: config.review_id.clone(),
            analytics: config.cost_tracker.to_snapshot().await,
        }
    );

    Ok(all_findings)
}

//TODO
async fn run_linters(_config: &EvalConfig) -> Vec<Finding> {
    // let mut linter_findings: Vec<Finding> = Vec::new();
    // let mut linter_set = tokio::task::JoinSet::new();
    // for (_, lconfig) in configs.iter() {
    //     let tool = create_linter_tool(lconfig);
    //     let args = LinterArgs {
    //         repo_path: config.repo_root.to_string_lossy().to_string(),
    //     };
    //     linter_set.spawn(async move {
    //         let result = tool.call(args).await;
    //         result
    //     });
    // }

    // while let Some(res) = linter_set.join_next().await {
    //     match res {
    //         Ok(Ok(findings)) => linter_findings.extend(findings),
    //         Ok(Err(e)) => warn!("Linter failed: {e}"),
    //         Err(e) => warn!("Linter join error: {e}"),
    //     }
    // }

    // info!(
    //     "Found {} linter finding(s) for repo {:?}",
    //     linter_findings.len(),
    //     config.repo_root
    // );

    // linter_findings
    todo!()
}

/// Run the configured agents on the given diff and return their findings along with any errors encountered during execution.
async fn run_reviewers(diff: &Diff, config: &EvalConfig) -> (Vec<Finding>, Option<ManyErrors>) {
    let effective_agents = get_agents_for_diff(diff, config.agents);
    if effective_agents.is_empty() {
        return (Vec::new(), None);
    }

    let tool_server = build_tool_server(config.context.repo_root.to_str()).run();
    let mut agent_set = task::JoinSet::new();

    let config = Arc::new(config.clone());
    for agent_entry in effective_agents {
        let tool_server_handle = tool_server.clone();
        let diff_str = diff.raw.clone();

        let config = config.clone();
        agent_set.spawn(async move {
            let agent_id = agent_entry.agent_id.clone();
            let agent = build_agent(config.clone(), agent_entry, tool_server_handle)
                .output_schema::<Vec<Finding>>()
                .build();

            stream_agent::<Vec<Finding>, _, _>(config, &agent_id, &agent, &diff_str).await
        });
    }

    let mut errors = None;
    let mut all_findings = Vec::<Finding>::new();
    while let Some(res) = agent_set.join_next().await {
        match res {
            Ok(Ok(findings)) => all_findings.extend(findings),
            Ok(Err(e)) => {
                let errors = errors.get_or_insert_with(ManyErrors::new);
                errors.push(e)
            }
            Err(e) => warn!("Agent join error: {e}"),
        }
    }

    (all_findings, errors)
}

fn post_process(findings: &[Finding], _config: &EvalConfig) -> Vec<Finding> {
    post_process_findings(findings)
}

async fn metrics(config: &EvalConfig) {
    let snapshot = config.cost_tracker.to_snapshot().await;
    let (total_in, total_out) = snapshot.total_tokens();
    info!(
        "Metrics: {} sessions, {} tokens in, {} tokens out, ${:.4} estimated cost",
        snapshot.sessions.len(),
        total_in,
        total_out,
        snapshot.total_cost(),
    );
}

async fn report(config: &EvalConfig) {
    let snapshot = config.cost_tracker.to_snapshot().await;
    info!(
        "Report: {} sessions, cache hit rate: {:.2}%",
        snapshot.sessions.len(),
        snapshot.hit_rate() * 100.0,
    );
}
