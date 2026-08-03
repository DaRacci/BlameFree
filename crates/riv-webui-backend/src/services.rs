use std::{env, fs};

use mti::prelude::MagicTypeId;
use riv_agents::prompts::PromptLibrary;
use riv_harness::model_capabilities::{available_models, supports_reasoning};
use riv_reporting::golden::load_golden_datasets;
use riv_stor::traits::Store;
use riv_types::{
    agent::{AgentSession, AgentTurn, AgentTurnMessage, RoleMessage, ToolInvocation},
    benchmark::{golden::GoldenCommentEntry, result::PrResult},
    capabilities::ReasoningEffort,
    review::{Review, ReviewStatus},
    vcs::pr::PrMeta,
    wrappers::Model,
};
use riv_webui_shared::{
    config::{AgentInfo, DatasetInfo},
    review::ReviewAgentLog,
};

use crate::server::AppState;

pub async fn list_reviews<S>(state: &AppState<S>) -> Result<Vec<Review>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let mut reviews = state
        .store
        .list::<Review>(&())
        .await
        .map_err(|error| format!("Failed to list reviews: {error}"))?;

    reviews.sort_by(|left, right| {
        let left_active = matches!(left.status, ReviewStatus::Pending | ReviewStatus::Running);
        let right_active = matches!(right.status, ReviewStatus::Pending | ReviewStatus::Running);

        right_active
            .cmp(&left_active)
            .then_with(|| right.id.to_string().cmp(&left.id.to_string()))
    });

    Ok(reviews)
}

pub async fn get_review<S>(state: &AppState<S>, review_id: &MagicTypeId) -> Result<Review, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    state
        .store
        .load::<Review>(review_id)
        .await
        .map_err(|error| format!("Failed to load review {review_id}: {error}"))?
        .ok_or_else(|| format!("Review {review_id} not found"))
}

pub async fn list_pr_results<S>(
    state: &AppState<S>,
    review_id: &MagicTypeId,
) -> Result<Vec<PrResult>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let review_key = review_id.to_string();
    let mut results = state
        .store
        .list::<PrResult>(&())
        .await
        .map_err(|error| format!("Failed to list PR results: {error}"))?
        .into_iter()
        .filter(|result| result_matches_review(result, review_id, &review_key))
        .collect::<Vec<_>>();

    results.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
    Ok(results)
}

pub async fn list_agent_logs<S>(
    state: &AppState<S>,
    review_id: &MagicTypeId,
) -> Result<Vec<ReviewAgentLog>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let review = get_review(state, review_id).await?;
    let mut logs = review
        .agent_sessions
        .iter()
        .map(|session| build_review_agent_log(review_id, session))
        .collect::<Vec<_>>();

    logs.sort_by(|left, right| left.agent_id.to_string().cmp(&right.agent_id.to_string()));
    Ok(logs)
}

pub async fn list_repo_prs<S>(
    state: &AppState<S>,
    owner: &str,
    repo: &str,
) -> Result<Vec<PrMeta>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let page = state
        .octocrab
        .pulls(owner, repo)
        .list()
        .state(octocrab::params::State::Open)
        .per_page(100)
        .send()
        .await
        .map_err(|error| format!("GitHub API error: {error}"))?;

    Ok(page
        .items
        .into_iter()
        .map(|pr| PrMeta {
            number: u32::try_from(pr.number).unwrap_or(u32::MAX),
            title: pr.title.unwrap_or_default(),
            url: pr.html_url.map(|url| url.to_string()).unwrap_or_default(),
        })
        .collect())
}

pub async fn fetch_pr_diff<S>(
    state: &AppState<S>,
    owner: &str,
    repo: &str,
    pr_number: u32,
) -> Result<(String, String), String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let pr = state
        .octocrab
        .pulls(owner, repo)
        .get(u64::from(pr_number))
        .await
        .map_err(|error| format!("Failed to fetch PR metadata: {error}"))?;

    let title = pr.title.unwrap_or_default();

    let diff_client = reqwest::Client::new();
    let token = env::var("GITHUB_TOKEN").ok();
    let mut diff_request = diff_client
        .get(format!(
            "https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}"
        ))
        .header("Accept", "application/vnd.github.v3.diff")
        .header("User-Agent", "blamefree-webui/1.0");

    if let Some(token) = token {
        diff_request = diff_request.header("Authorization", format!("Bearer {token}"));
    }

    let diff = diff_request
        .send()
        .await
        .map_err(|error| format!("Failed to fetch PR diff: {error}"))?
        .text()
        .await
        .map_err(|error| format!("Failed to read diff text: {error}"))?;

    Ok((title, diff))
}

pub async fn list_datasets<S>(state: &AppState<S>) -> Result<Vec<DatasetInfo>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let dataset_dir = &state.config.server.dataset_dir;
    if !dataset_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(dataset_dir).map_err(|error| {
        format!(
            "Failed to read dataset directory {}: {error}",
            dataset_dir.display()
        )
    })?;

    let mut datasets = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            continue;
        }

        let pr_count = load_golden_datasets(&path)
            .map(|entries| entries.len())
            .unwrap_or_default();

        datasets.push(DatasetInfo {
            id,
            path: path.to_string_lossy().to_string(),
            pr_count,
        });
    }

    datasets.sort_by(|left, right| {
        right
            .pr_count
            .cmp(&left.pr_count)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(datasets)
}

pub async fn list_dataset_prs<S>(
    state: &AppState<S>,
    dataset_id: &str,
) -> Result<Vec<GoldenCommentEntry>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let dataset_dir = state.config.server.dataset_dir.join(dataset_id);
    if !dataset_dir.exists() || !dataset_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut prs = load_golden_datasets(&dataset_dir)
        .map_err(|error| format!("Failed to load dataset {dataset_id}: {error}"))?;

    prs.sort_by(|left, right| {
        extract_pr_number(&left.url)
            .cmp(&extract_pr_number(&right.url))
            .then_with(|| left.pr_title.cmp(&right.pr_title))
    });
    Ok(prs)
}

pub async fn list_models<S>(_state: &AppState<S>) -> Result<Vec<Model>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    Ok(available_models())
}

pub async fn list_reasoning_efforts<S>(
    _state: &AppState<S>,
    model: &str,
) -> Result<Vec<ReasoningEffort>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    if !supports_reasoning(&Model(model.to_string())) {
        return Ok(Vec::new());
    }

    Ok(vec![
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::XHigh,
        ReasoningEffort::Max,
    ])
}

pub async fn list_agents<S>(_state: &AppState<S>) -> Result<Vec<AgentInfo>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let library = PromptLibrary::new().map_err(|error| error.to_string())?;
    let mut abbreviations = library
        .abbreviations()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    abbreviations.sort();

    Ok(abbreviations
        .into_iter()
        .filter_map(|abbreviation| {
            library.config(&abbreviation).map(|entry| AgentInfo {
                name: entry.role_name.clone(),
                abbreviation,
                incompatible_with_roles: entry.incompatible_with_roles.clone(),
            })
        })
        .collect())
}

fn result_matches_review(result: &PrResult, review_id: &MagicTypeId, review_key: &str) -> bool {
    result.id == *review_id
        || result.benchmark_id.as_ref() == Some(review_id)
        || result.id.to_string().starts_with(review_key)
}

fn build_review_agent_log(review_id: &MagicTypeId, session: &AgentSession) -> ReviewAgentLog {
    let messages = ordered_messages(session);

    let prompt = messages
        .iter()
        .filter_map(|message| match message {
            RoleMessage::System(text) | RoleMessage::User(text) => Some(text.clone()),
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let response = messages
        .iter()
        .filter_map(|message| match message {
            RoleMessage::Assistant(response) => Some(response.output.clone()),
            RoleMessage::Tool(invocation) => Some(format_tool_message(invocation)),
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let reasoning = messages
        .iter()
        .filter_map(|message| match message {
            RoleMessage::Assistant(response) => Some(response.thinking.clone()),
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    ReviewAgentLog {
        review_id: review_id.clone(),
        agent_id: session.id.clone(),
        model_name: session.model_name.clone(),
        prompt,
        response,
        reasoning,
    }
}

fn ordered_messages(session: &AgentSession) -> Vec<RoleMessage> {
    let mut turns: Vec<AgentTurn> = session.turns.clone();
    turns.sort_by_key(|turn| turn.turn_index);

    turns
        .into_iter()
        .flat_map(|turn| {
            let mut messages: Vec<AgentTurnMessage> = turn.messages;
            messages.sort_by_key(|message| message.msg_index);
            messages.into_iter().map(RoleMessage::from)
        })
        .collect()
}

fn format_tool_message(invocation: &ToolInvocation) -> String {
    let input = serde_json::to_string_pretty(&invocation.input)
        .unwrap_or_else(|_| invocation.input.to_string());
    let output = serde_json::to_string_pretty(&invocation.output)
        .unwrap_or_else(|_| invocation.output.to_string());

    let mut sections = vec![format!("[tool] {}", invocation.tool_name)];
    if !input.trim().is_empty() && input != "null" {
        sections.push(format!("input:\n{input}"));
    }
    if !output.trim().is_empty() && output != "null" {
        sections.push(format!("output:\n{output}"));
    }

    sections.join("\n")
}

fn extract_pr_number(url: &str) -> u32 {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|segment| segment.parse::<u32>().ok())
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use riv_types::agent::AgentResponse;

    #[test]
    fn test_extract_pr_number() {
        assert_eq!(extract_pr_number("https://github.com/a/b/pull/42"), 42);
        assert_eq!(extract_pr_number("not-a-pr"), u32::MAX);
    }

    #[test]
    fn test_build_review_agent_log() {
        let review_id = MagicTypeId::default();
        let session_id = MagicTypeId::default();
        let session = AgentSession {
            id: session_id.clone(),
            review_id: Some(review_id.clone()),
            model_name: "openai/gpt-5-mini".to_string(),
            turns: vec![AgentTurn {
                id: None,
                session_id: session_id,
                turn_index: 0,
                messages: vec![
                    RoleMessage::System("System prompt".to_string()).into(),
                    RoleMessage::User("Review this diff".to_string()).into(),
                    RoleMessage::Assistant(AgentResponse {
                        thinking: "Need inspect auth flow".to_string(),
                        output: "Found potential bug".to_string(),
                    })
                    .into(),
                    AgentTurnMessage {
                        id: None,
                        turn_id: 0,
                        msg_index: 3,
                        role: "tool".to_string(),
                        text_content: None,
                        thinking: None,
                        output: None,
                        tool_name: Some("grep".to_string()),
                        tool_input: Some("{\"regex\":\"auth\"}".to_string()),
                        tool_output: Some("[\"src/auth.rs\"]".to_string()),
                    },
                ],
            }],
        };

        let log = build_review_agent_log(&review_id, &session);
        assert!(log.prompt.contains("System prompt"));
        assert!(log.prompt.contains("Review this diff"));
        assert!(log.response.contains("Found potential bug"));
        assert!(log.response.contains("[tool] grep"));
        assert!(log.reasoning.contains("Need inspect auth flow"));
    }
}
