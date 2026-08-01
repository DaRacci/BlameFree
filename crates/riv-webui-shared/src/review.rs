use riv_types::agent::{AgentSession, RoleMessage};
use riv_types::vcs::pr::PrMeta;
use serde::{Deserialize, Serialize};

use crate::config::AgentInfo;

pub use riv_types::review::PullRequestReviewMetadata;
pub use riv_types::review::Review;
pub use riv_types::review::ReviewStatus;

/// Run config returned in the run detail response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated]
pub struct RunConfig {
    /// Model used for the run.
    pub model: String,

    /// Dataset identifier.
    pub dataset: String,

    /// Reviewer agents.
    pub agents: Vec<AgentInfo>,
}

/// Response from GET /api/runs/:id/logs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated]
pub struct LogsListResponse {
    /// Run ID for this log response.
    pub run_id: String,

    /// Per-PR log entries.
    pub prs: Vec<PrLogsEntry>,
}

/// A single PR's available log entries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated]
pub struct PrLogsEntry {
    /// PR Details.
    pub meta: PrMeta,

    /// Agent roles available for this PR.
    pub agents: Vec<AgentInfo>,
}

/// Response from GET /api/runs/:id/logs/:pr_key/:role
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated]
pub struct AgentLogResponse {
    /// Run ID.
    pub run_id: String,

    /// The prompt sent to the agent, if available.
    pub prompt: Option<String>,

    /// The agent's response, if available.
    pub response: Option<String>,

    /// Reasoning text, if available.
    pub reasoning: Option<String>,

    /// Whether this log entry is accessible.
    pub available: bool,
}

/// Response from GET /api/runs/:id/prs/:pr_key
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated]
pub struct PrAgentsResponse {
    /// Run ID.
    pub run_id: String,

    /// PR key.
    pub pr_key: String,

    /// PR title.
    pub pr_title: String,

    /// Per-agent availability list.
    pub agents: Vec<PrAgentEntry>,

    /// Whether any agent output exists.
    pub has_output: bool,
}

/// Per-agent availability entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated]
pub struct PrAgentEntry {
    /// Role abbreviation.
    pub role: String,

    /// Whether a prompt is available for this agent.
    pub has_prompt: bool,

    /// Whether a response is available for this agent.
    pub has_response: bool,

    /// Whether reasoning text is available for this agent.
    pub has_reasoning: bool,
}

impl From<(&str, &AgentSession)> for AgentLogResponse {
    /// Build an [`AgentLogResponse`] from a run-id and an [`AgentSession`].
    ///
    /// Extracts the first `User` message as the prompt, and the most recent
    /// `Assistant` message's `output` / `thinking` as response / reasoning.
    fn from((run_id, session): (&str, &AgentSession)) -> Self {
        let all_messages: Vec<RoleMessage> = session
            .turns
            .iter()
            .flat_map(|t| t.messages.iter())
            .cloned()
            .map(RoleMessage::from)
            .collect();

        let turns: Vec<&RoleMessage> = all_messages.iter().collect();

        let prompt = turns.iter().find_map(|m| {
            if let RoleMessage::User(p) = m {
                Some(p.clone())
            } else {
                None
            }
        });

        let (response, reasoning) = turns
            .iter()
            .rev()
            .find_map(|m| {
                if let RoleMessage::Assistant(r) = m {
                    Some((r.output.clone(), r.thinking.clone()))
                } else {
                    None
                }
            })
            .map(|(resp, think)| {
                let think_trimmed = think.trim();
                let reasoning = if think_trimmed.is_empty() {
                    None
                } else {
                    Some(think_trimmed.to_string())
                };
                (Some(resp), reasoning)
            })
            .unwrap_or((None, None));

        let available = prompt.is_some() || response.is_some() || reasoning.is_some();

        AgentLogResponse {
            run_id: run_id.to_string(),
            prompt,
            response,
            reasoning,
            available,
        }
    }
}

impl From<(&str, &AgentSession)> for PrAgentEntry {
    /// Build a [`PrAgentEntry`] from a role abbreviation and an [`AgentSession`].
    ///
    /// Scans the session's turns to determine whether prompt/response/reasoning
    /// data is available.
    fn from((role, session): (&str, &AgentSession)) -> Self {
        let all_messages: Vec<RoleMessage> = session
            .turns
            .iter()
            .flat_map(|t| t.messages.iter())
            .cloned()
            .map(RoleMessage::from)
            .collect();

        let turns: Vec<&RoleMessage> = all_messages.iter().collect();

        let has_prompt = turns.iter().any(|m| matches!(m, RoleMessage::User(_)));
        let has_response = turns.iter().any(|m| matches!(m, RoleMessage::Assistant(_)));
        let has_reasoning = turns.iter().any(|m| {
            if let RoleMessage::Assistant(r) = m {
                !r.thinking.trim().is_empty()
            } else {
                false
            }
        });

        PrAgentEntry {
            role: role.to_string(),
            has_prompt,
            has_response,
            has_reasoning,
        }
    }
}

impl From<&AgentSession> for PrAgentEntry {
    /// Build a [`PrAgentEntry`] from an [`AgentSession`] with an empty role
    /// placeholder.
    ///
    /// Use [`From<(&str, &AgentSession)>`] when the role abbreviation is known.
    fn from(session: &AgentSession) -> Self {
        Self::from(("", session))
    }
}

/// Build a [`PrLogsEntry`] from a [`PrMeta`] and a reference to a
/// `HashMap<MagicTypeId, AgentSession>`.
///
/// The agent list is derived from the session keys (using their string
/// representation as a fallback abbreviation).  Prefer the file-backed
/// approach when full [`AgentInfo`] (name, abbreviation) is required.
impl
    From<(
        PrMeta,
        &std::collections::HashMap<mti::prelude::MagicTypeId, AgentSession>,
    )> for PrLogsEntry
{
    fn from(
        (meta, sessions): (
            PrMeta,
            &std::collections::HashMap<mti::prelude::MagicTypeId, AgentSession>,
        ),
    ) -> Self {
        use std::collections::BTreeSet;
        let mut seen = BTreeSet::new();
        let agents: Vec<AgentInfo> = sessions
            .keys()
            .filter(|id| seen.insert(id.to_string()))
            .map(|id| AgentInfo {
                name: id.to_string(),
                abbreviation: id.to_string(),
                incompatible_with_roles: vec![],
            })
            .collect();
        PrLogsEntry { meta, agents }
    }
}

/// Build a [`PrAgentsResponse`] from run metadata and agent sessions.
///
/// Each agent session is converted to a [`PrAgentEntry`] using the session's
/// ID string as the role placeholder.
impl
    From<(
        &str,
        &str,
        &str,
        &std::collections::HashMap<mti::prelude::MagicTypeId, AgentSession>,
    )> for PrAgentsResponse
{
    fn from(
        (run_id, pr_key, pr_title, sessions): (
            &str,
            &str,
            &str,
            &std::collections::HashMap<mti::prelude::MagicTypeId, AgentSession>,
        ),
    ) -> Self {
        let agents: Vec<PrAgentEntry> = sessions
            .iter()
            .map(|(id, session)| {
                let role = id.to_string();
                PrAgentEntry::from((role.as_str(), session))
            })
            .collect();
        let has_output = agents.iter().any(|a| a.has_prompt || a.has_response);
        PrAgentsResponse {
            run_id: run_id.to_string(),
            pr_key: pr_key.to_string(),
            pr_title: pr_title.to_string(),
            agents,
            has_output,
        }
    }
}
