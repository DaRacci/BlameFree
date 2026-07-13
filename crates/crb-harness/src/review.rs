use crb_shared::finding::Finding;
use rig_core::providers::openai;

use crate::{config::ReviewArgs, eval::EvalConfig, run_agent_roles};

/// Entry point for reviewing a PR given its diff as a string.
///
/// Builds agents for each role, runs them with the diff, and returns findings.
pub async fn review_pr(
    config: &EvalConfig,
    tool_server_handle: ToolServerHandle,
) -> Result<Vec<Finding>> {
    let client =
        openai::Client::from_env().map_err(|e| anyhow!("Failed to create OpenAI client: {e}"))?;

    let roles: Vec<&str> = if config.roles.is_empty() {
        PromptLibrary::get_instance().abbreviations()
    } else {
        config.roles.iter().map(|r| r.as_str()).collect()
    };

    let findings = run_agent_roles(
        &client,
        &config.model,
        &config.diff,
        &roles,
        params.max_findings,
        tool_server_handle,
    )
    .await;

    let findings = post_process_findings(&findings);
    Ok(findings)
}

/// Review a diff by running `git diff` in the given `path`,
/// then call `review_pr()` with the diff to get agent findings.
///
/// - `ReviewMode::Commits { base, head }` -> `git diff base..head`
/// - `ReviewMode::Working`                -> `git diff` (unstaged + staged)
///
/// Returns a vector of agent findings parsed from the LLM response.
pub async fn review_diff(args: ReviewArgs) -> Result<Vec<Finding>> {
    let tool_server = build_tool_server(args.path.to_str(), None).run();

    let diff = {
        let cmd_args = if let Some(ref range) = args.commits {
            vec!["diff", range]
        } else {
            vec!["diff"]
        };

        let output = Command::new("git")
            .args(cmd_args)
            .current_dir(&args.path)
            .output()
            .map_err(|e| anyhow!("Failed to run git diff: {e}"))?;
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    if diff.is_empty() {
        info!("No diff found; returning empty findings");
        return Ok(Vec::new());
    }

    info!(
        "Loaded diff ({} bytes) from {}",
        diff.len(),
        args.path.display()
    );

    let diff = diff::preprocess_diff(Diff::new(diff));

    let roles = PromptLibrary::get_instance().abbreviations();
    let params = ReviewParams {
        diff: diff.clone(),
        model: args.model.clone(),
        pr_title: "review".to_string(),
        roles: roles.iter().map(|s| s.to_string()).collect(),
        max_findings: 20,
        cache_dir: None,
    };
    review_pr(params, tool_server).await
}
