use std::sync::{Arc, LazyLock};

use anyhow::Result;
use crb_agents::build_agent;
use crb_shared::{diff::Diff, finding::Finding};
use rig_core::{
    agent::PromptResponse,
    completion::Prompt,
};
use tokio::{
    sync::Semaphore,
    task::JoinSet,
};
use tracing::{error, info};

use crate::eval::EvalConfig;

const MAX_CONCURRENT_AGENTS: usize = 4;
static SEMAPHORE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(MAX_CONCURRENT_AGENTS)));

#[deprecated = "Use pipeline::run_reviewers() instead."]
/// Run the shared agent loop for a set of roles, collecting findings.
async fn run_agents(diff: &Diff, config: &EvalConfig) -> Vec<Finding> {
    let client = config.client.clone();
    let model = config.model.clone();
    let tool_server_handle = config.tool_handle.clone();
    let max_findings = config.max_findings;
    let diff_str = diff.raw.clone();
    let agents = config.agents.to_vec();

    let results = run_concurrent(agents, move |entry| {
        let client = client.clone();
        let model = model.clone();
        let tool_server_handle = tool_server_handle.clone();
        let diff_str = diff_str.clone();
        let entry_name = entry.role_abbreviation.clone();

        async move {
            let agent = build_agent(
                &client,
                &model,
                entry,
                None,
                None,
                None,
                None,
                tool_server_handle.clone(),
            )
            .output_schema::<Vec<Finding>>()
            .build();

            let resp: PromptResponse = agent
                .prompt(diff_str)
                .extended_details()
                .await
                .map_err(|e| anyhow::anyhow!("Agent {entry_name} failed: {e}"))?;

            let mut findings: Vec<Finding> =
                serde_json::from_str(&resp.output).unwrap_or_default();
            if findings.len() > max_findings {
                info!(
                    "Agent {entry_name} produced {} findings, capping at {max_findings}",
                    findings.len(),
                );
                findings.truncate(max_findings);
            }

            Ok(findings)
        }
    })
    .await;

    // Flatten collected findings
    let mut all_findings = Vec::new();
    for findings in results {
        all_findings.extend(findings);
    }

    all_findings
}

/// Run a concurrent task set over a list of items.
///
/// Spawns up to [`MAX_CONCURRENT_AGENTS`] tasks at a time, each calling `task_fn`.
async fn run_concurrent<T, R, F, Fut>(items: Vec<T>, task_fn: F) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<R>> + Send,
{
    let task_fn = Arc::new(task_fn);
    let total = items.len();
    let mut set = JoinSet::new();
    for item in items {
        let sem = SEMAPHORE.clone();
        let permit = sem
            .acquire_owned()
            .await
            .expect("Failed to acquire semaphore permit");
        let task_fn = task_fn.clone();
        set.spawn(async move {
            let _permit = permit;
            task_fn(item).await
        });
    }

    let mut results = Vec::with_capacity(total.min(MAX_CONCURRENT_AGENTS));
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => error!("Task error: {e}"),
            Err(e) => error!("Join error: {e}"),
        }
    }

    results
}
