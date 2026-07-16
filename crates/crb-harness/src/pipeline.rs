use anyhow::Result;
use crb_agents::build_agent;
use crb_consensus::adaptive::get_agents_for_diff;
use crb_reporting::PrResult;
use crb_shared::{
    diff::{self, Diff},
    finding::Finding,
    sanitize_filename,
};
use crb_tools::{build_tool_server, linters::create_linter_tool, linters::tool::LinterArgs};
use crb_types::RunEvent;
use crb_types::benchmark::{Metrics, MetricsProvider};
use rig_core::completion::Prompt;
use rig_core::tool::Tool;
use tokio::task;
use tracing::{info, warn};

use crate::eval::EvalConfig;
use crate::finding::post_process_findings;

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
pub async fn send_agent_started_events(config: &EvalConfig, identifier: &str) {
    if let Some(ref tx) = config.dashboard_tx {
        let pr_key = sanitize_filename(identifier);
        for entry in config.agents {
            let _ = tx.send(RunEvent::AgentStarted {
                identifier: pr_key.clone(),
                agent: entry.role_abbreviation.to_string(),
            });
        }
    }
}

/// Build a PrResult from evaluation outputs and caller-provided PR metadata.
pub async fn build_pr_result(
    findings: &[Finding],
    config: &EvalConfig,
    pr_title: &str,
    url: &str,
    golden_count: usize,
) -> PrResult {
    let snapshot = config.cost_tracker.to_snapshot().await;
    let metrics = Metrics::default();
    PrResult {
        pr_title: pr_title.to_string(),
        url: url.to_string(),
        findings_count: findings.len(),
        golden_count,
        metrics,
        verdicts: Vec::new(),
        cost: Some(snapshot),
    }
}

pub async fn evaluate(mut diff: Diff, config: &EvalConfig) -> Result<Vec<Finding>> {
    send_event!(config, RunEvent::ReviewStarted {
        identifier: config.identifier.clone(),
        total_agents: config.agents.len(),
    });

    diff::preprocess_diff(&mut diff);

    let linters = run_linters(config);
    let reviewers = run_reviewers(&diff, config);
    let (mut all_findings, reviewer_findings) = tokio::join!(linters, reviewers);

    let reviewer_findings = post_process(reviewer_findings.as_slice(), config);
    all_findings.extend(reviewer_findings);

    let snapshot = config.cost_tracker.to_snapshot().await;
    let (total_tokens_in, total_tokens_out) = snapshot.total_tokens().await;

    metrics(config).await;
    report(config).await;

    let findings_count = all_findings.len();
    let agent_calls = config.agents.len();

    send_event!(config, RunEvent::ReviewCompleted {
        identifier: config.identifier.clone(),
        metrics: crb_types::benchmark::Metrics::default(),
        cost: snapshot.total_cost(),
        total_tokens: (total_tokens_in + total_tokens_out) as usize,
        agent_calls,
        findings_count,
    });

    Ok(all_findings)
}

async fn run_linters(config: &EvalConfig) -> Vec<Finding> {
    let mut linter_findings: Vec<Finding> = Vec::new();
    if let Some(ref configs) = config.linter_configs {
        let mut linter_set = tokio::task::JoinSet::new();
        for (_, lconfig) in configs.iter() {
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

    linter_findings
}

async fn run_reviewers(diff: &Diff, config: &EvalConfig) -> Vec<Finding> {
    let effective_agents = get_agents_for_diff(diff, config.agents);
    if effective_agents.is_empty() {
        return Vec::new();
    }

    let tool_server = build_tool_server(config.repo_root.to_str()).run();
    let mut agent_set = task::JoinSet::new();

    for agent_entry in effective_agents {
        let tool_server_handle = tool_server.clone();
        let diff_str = diff.raw.clone();
        let client = config.client.clone();
        let model = config.model.clone();
        let rules_preamble = config.ruleset.as_ref().map(|rs| rs.format_preamble(&[]));
        let template_vars = config.template_vars.clone();
        let max_findings = config.max_findings;

        agent_set.spawn(async move {
            let agent = build_agent(
                &client,
                &model,
                agent_entry,
                rules_preamble.as_deref(),
                template_vars.as_ref(),
                None,
                None,
                tool_server_handle,
            )
            .output_schema::<Vec<Finding>>()
            .build();

            match agent.prompt(diff_str).extended_details().await {
                Ok(resp) => {
                    let findings: Vec<Finding> =
                        serde_json::from_str(&resp.output).unwrap_or_default();
                    let mut findings = findings;
                    if findings.len() > max_findings {
                        info!(
                            "Agent {} produced {} findings, capping at {max_findings}",
                            agent_entry.role_abbreviation,
                            findings.len(),
                        );
                        findings.truncate(max_findings);
                    }
                    findings
                }
                Err(e) => {
                    warn!("Agent {} failed: {e}", agent_entry.role_abbreviation);
                    Vec::new()
                }
            }
        });
    }

    let mut all_findings = Vec::new();
    while let Some(res) = agent_set.join_next().await {
        match res {
            Ok(findings) => all_findings.extend(findings),
            Err(e) => warn!("Agent join error: {e}"),
        }
    }

    all_findings
}

fn post_process(findings: &[Finding], _config: &EvalConfig) -> Vec<Finding> {
    post_process_findings(findings)
}

async fn metrics(config: &EvalConfig) {
    let snapshot = config.cost_tracker.to_snapshot().await;
    let (total_in, total_out) = snapshot.total_tokens().await;
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

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use crb_agents::agent::AgentEntry;
    use crb_reporting::cost::AnalyticsTracker;
    use crb_shared::finding::Finding;
    use crb_types::wrappers::Model;
    use rig_core::client::CompletionClient;
    use rig_core::tool::server::ToolServer;

    use crate::eval::{EvalConfig, EvalStrategy};

    use super::*;

    fn build_minimal_config() -> EvalConfig {
        let tool_server = ToolServer::new().run();

        let client = Arc::new(
            rig_core::providers::openai::Client::builder()
                .api_key("test-key")
                .build()
                .unwrap(),
        );

        EvalConfig {
            strategy: EvalStrategy::Panel,
            identifier: "test-run".to_string(),
            model: Model("test-model".to_string()),
            reasoning_effort: None,
            client: client.clone(),
            cache: None,
            cost_tracker: Arc::new(AnalyticsTracker::new()),
            tool_handle: tool_server,
            dashboard_tx: None,
            agents: &[],
            repo_root: PathBuf::from("/tmp/test"),
            max_findings: 20,
            judge_model: "judge-model".to_string(),
            judge: client.as_ref().agent("judge-model").preamble("You are a test judge.").build(),
            linters_only: false,
            linter_configs: None,
            ruleset: None,
            template_vars: None,
        }
    }

    #[tokio::test]
    async fn test_build_pr_result_empty_findings() {
        let config = build_minimal_config();
        let result = build_pr_result(&[], &config, "Test PR", "https://example.com/pr/1", 0).await;

        assert_eq!(result.pr_title, "Test PR");
        assert_eq!(result.url, "https://example.com/pr/1");
        assert_eq!(result.findings_count, 0);
        assert_eq!(result.golden_count, 0);
        assert!(result.verdicts.is_empty());
        assert!(result.cost.is_some());
    }

    #[tokio::test]
    async fn test_build_pr_result_with_findings() {
        let config = build_minimal_config();
        let findings = vec![
            Finding {
                file: Some("src/main.rs".to_string()),
                line: Some(10),
                message: "Test finding".to_string(),
                severity: crb_shared::severity::Severity::Medium,
                evidence: None,
                rule_code: None,
                severity_audited: false,
                severity_audit_reason: None,
                path_trace: None,
                confidence: None,
                found_by: None,
                agent_count: None,
                cross_validated: false,
                cross_validated_by: None,
                merged_from: None,
            },
        ];

        let result = build_pr_result(&findings, &config, "Test PR", "https://example.com/pr/2", 3).await;

        assert_eq!(result.findings_count, 1);
        assert_eq!(result.golden_count, 3);
        assert_eq!(result.pr_title, "Test PR");
    }

    #[tokio::test]
    async fn test_build_pr_result_with_cost() {
        let config = build_minimal_config();
        let result = build_pr_result(&[], &config, "Cost Test", "https://example.com/pr/3", 0).await;

        let cost = result.cost.unwrap();
        // Fresh cost tracker should have zero sessions and zero cost
        assert_eq!(cost.sessions.len(), 0);
        assert_eq!(cost.total_cost(), 0.0);
    }
}
