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
}

impl ReviewAgentLog {
    pub fn available(&self) -> bool {
        !self.prompt.trim().is_empty()
            || !self.response.trim().is_empty()
            || !self.reasoning.trim().is_empty()
    }
}
