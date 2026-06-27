//! Subprocess management for launching and monitoring crb-harness.
//!
//! The backend spawns `crb-harness --dashboard-events` as a subprocess,
//! reads JSON events from its stdout line-by-line, and forwards them
//! to all SSE clients via a broadcast channel.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::{broadcast, RwLock};

use crate::api::BenchmarkConfig;
use crate::events::{parse_event_line, DashboardEvent};
use crate::server::ActiveRun;

/// Run the harness as a subprocess and forward events to the broadcast channel.
///
/// This function:
/// 1. Spawns `crb-harness --dashboard-events [config args]`
/// 2. Reads stdout line-by-line, parsing JSON events
/// 3. Forwards parsed events to all SSE clients via `tx`
/// 4. Updates `active_runs` state with progress
/// 5. Cleans up on completion or error
pub async fn run_harness(
    harness_path: &Path,
    run_id: &str,
    config: &BenchmarkConfig,
    output_dir: &Path,
    tx: broadcast::Sender<DashboardEvent>,
    active_runs: Arc<RwLock<HashMap<String, ActiveRun>>>,
) -> anyhow::Result<()> {
    let output_subdir = output_dir.join(run_id);

    let mut cmd = Command::new(harness_path);
    cmd.arg("--dashboard-events")
        .arg("--model")
        .arg(&config.model)
        .arg("--judge-model")
        .arg(&config.judge_model)
        .arg("--dataset-dir")
        .arg(&config.dataset_dir)
        .arg("--concurrency")
        .arg(config.concurrency.to_string())
        .arg("--max-findings")
        .arg(config.max_findings.to_string())
        .arg("--prompts-dir")
        .arg(&config.prompts_dir)
        .arg("--roles")
        .arg(&config.roles)
        .arg("--output-dir")
        .arg(output_subdir.to_string_lossy().to_string());

    if config.skip_consensus {
        cmd.arg("--skip-consensus");
    }
    if config.skip_linters {
        cmd.arg("--skip-linters");
    }
    if let Some(ref cache_dir) = config.cache_dir {
        cmd.arg("--cache-dir").arg(cache_dir);
    }
    if let Some(ref pr_filter) = config.pr_filter {
        cmd.arg("--pr-filter").arg(pr_filter);
    }

    // Set up stdout for reading JSON events
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit()); // Keep stderr visible for debugging

    let mut child = cmd.spawn().map_err(|e| {
        anyhow::anyhow!("Failed to spawn crb-harness: {e}")
    })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        anyhow::anyhow!("Failed to capture harness stdout")
    })?;

    let reader = tokio::io::BufReader::new(stdout);
    let mut lines = reader.lines();

    let start_time = std::time::Instant::now();

    while let Some(line) = lines.next_line().await? {
        if let Some(event) = parse_event_line(&line) {
            // Update active run state
            match &event {
                DashboardEvent::RunProgress {
                    completed_prs,
                    total_prs,
                    ..
                } => {
                    let mut runs = active_runs.write().await;
                    if let Some(run) = runs.get_mut(run_id) {
                        run.completed_prs = *completed_prs;
                        run.total_prs = *total_prs;
                    }
                }
                DashboardEvent::RunFinished { .. } => {
                    let mut runs = active_runs.write().await;
                    if let Some(run) = runs.get_mut(run_id) {
                        run.finished = true;
                    }
                }
                _ => {}
            }

            // Broadcast to all SSE clients (ignore send errors if no clients)
            let _ = tx.send(event);
        }
    }

    // Wait for the child process to exit
    let status = child.wait().await?;

    // Update run as finished
    {
        let mut runs = active_runs.write().await;
        if let Some(run) = runs.get_mut(run_id) {
            run.finished = true;
        }
    }

    let elapsed = start_time.elapsed();
    tracing::info!(
        "Harness run {} finished (status: {}, elapsed: {:.1}s)",
        run_id,
        status,
        elapsed.as_secs_f64()
    );

    // Send final event
    let _ = tx.send(DashboardEvent::RunFinished {
        total_prs: 0,
        aggregated: Default::default(),
        total_cost: 0.0,
        total_tokens: 0,
        total_agent_calls: 0,
    });

    Ok(())
}
