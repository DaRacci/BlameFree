use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

pub use riv_types::review::PullRequestReviewMetadata;
pub use riv_types::review::Review;
pub use riv_types::review::ReviewStatus;

/// Flattened log view for a single agent session on a review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewAgentLog {
    pub review_id: MagicTypeId,
    pub agent_id: MagicTypeId,
    pub model_name: String,
    pub prompt: String,
    pub response: String,
    pub reasoning: String,
    /// Total number of input tokens for this agent session.
    pub input_tokens: u64,
    /// Total number of output tokens for this agent session.
    pub output_tokens: u64,
    /// Total number of tokens served from cache.
    pub cached_input_tokens: u64,
    /// Total number of tokens used to create a cache entry.
    pub cache_creation_input_tokens: u64,
    /// Total number of tokens used for reasoning.
    pub reasoning_tokens: u64,
    /// Total number of tokens used for tool use prompts.
    pub tool_use_prompt_tokens: u64,
    /// Total number of calls made by the agent.
    pub call_count: u64,
    /// Total number of tool calls made by the agent.
    pub tool_use_count: u64,
}

impl ReviewAgentLog {
    pub fn available(&self) -> bool {
        !self.prompt.trim().is_empty()
            || !self.response.trim().is_empty()
            || !self.reasoning.trim().is_empty()
    }
}
