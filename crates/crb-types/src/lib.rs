//! Shared event types for code review benchmark runs.

pub mod agent;
pub mod benchmark;
pub mod capabilities;
pub mod cost;
pub mod errors;
pub mod finding;
pub mod review;
pub mod severity;
pub mod vcs;
pub mod wrappers;

use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

use crate::{
    agent::AgentChunk,
    cost::{AnalyticsSnapshot, SessionUsage},
};

/// Events for the entire lifecycle of a review.
///
/// Serialized with a JSON tag/envelope format suitable for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "event", content = "data")]
pub enum RunEvent {
    /// An [`crate::agent::AgentSession`] has started running on a Review.
    AgentStarted {
        /// The [`crate::review::Review::id`] of this event.
        review_id: MagicTypeId,

        /// The [`crate::agent::AgentSession::id`] of this event.
        agent_id: MagicTypeId,
    },

    /// A chunk of streaming response text from an agent.
    AgentChunk {
        /// The [`crate::review::Review::id`] of this event.
        review_id: MagicTypeId,

        chunk: AgentChunk,
    },

    /// An [`crate::agent::AgentSession`] has finished its review.
    AgentFinished {
        /// The [`crate::review::Review::id`] of this event.
        review_id: MagicTypeId,

        /// The [`crate::agent::AgentSession::id`] of this event.
        agent_id: MagicTypeId,

        /// The final [`crate::cost::SessionUsage`] for this agent.
        analytics: SessionUsage,
    },

    /// A [`crate::review::Review`] has started running.
    ReviewStarted {
        /// The [`crate::review::Review::id`] of this event.
        review_id: MagicTypeId,

        /// The [`crate::agent::AgentSession::id`]'s of the agents that will run on this review.
        agent_ids: Vec<MagicTypeId>,
    },

    /// A [`crate::review::Review`] has finished running.
    ReviewCompleted {
        /// The [`crate::review::Review::id`] of this event.
        review_id: MagicTypeId,

        /// The snapshot of the final [`crate::cost::AnalyticsSnapshot`] for this review.
        analytics: AnalyticsSnapshot,
    },
}
