use std::{future::Future, sync::Arc, time::Instant};

use anyhow::Result;
use crb_agents::build_agent;
use crb_shared::{diff::Diff, finding::Finding};
use crb_types::RunEvent;
use rig_core::{
    completion::{TypedPrompt, request},
    streaming::{StreamingChat, StreamingCompletion, StreamingPrompt},
};
use tokio::{
    sync::{self, Semaphore},
    task::JoinSet,
};
use tracing::error;

use crate::{eval::EvalConfig, send_event};

const MAX_CONCURRENT_AGENTS: usize = 4;
static SEMAPHORE: Arc<Semaphore> = Arc::new(Semaphore::const_new(4));

/// Run the shared agent loop for a set of roles, collecting findings.
async fn run_agents(diff: &Diff, config: &EvalConfig) -> Vec<Finding> {
    let mut all_findings = Vec::new();

    config.cache.store_raw(key, value);

    let client = config.client;
    let model = config.model;
    let tool_server_handle = config.tool_handle;
    let config = Arc::new(config);
    run_concurrent(config.agents.to_vec(), async move |entry| {
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

        let mut stream = agent.stream_prompt(todo!("SEND THE DIFF")).await;
        while let Ok(msg) = stream.next().await {
            match msg {
                StreamingCompletion::Response(resp) => {
                    send_event!(RunEvent::AgentChunk {
                        identifier: entry.role_abbreviation.clone(),
                        chunk: ()
                    })
                }
                StreamingCompletion::Error(err) => {
                    send_event!(RunEvent::AgentFinished {
                        identifier: entry.role_abbreviation.clone(),
                        findings: (),
                        success: ()
                    })
                }
                StreamingCompletion::Done => {
                    send_event!(RunEvent::AgentFinished {
                        identifier: entry.role_abbreviation.clone(),
                        findings: (),
                        success: ()
                    })
                }
            }
        }

        match agent.prompt(diff).extended_details().await {
            Ok(resp) => {
                let response = resp.output;
                let _usage = resp.usage;
                match parse_agent_findings(&response) {
                    Ok(mut findings) => {
                        if findings.len() > max_findings {
                            findings.truncate(max_findings);
                        }
                        all_findings.append(&mut findings);
                    }
                    Err(e) => warn!("Failed to parse agent response for role {}: {}", role, e),
                }
            }
            Err(e) => warn!("Agent call failed for role {}: {}", role, e),
        }
    });

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
        let permit = SEMAPHORE.clone().acquire_owned().await?;
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
