use std::{collections::HashMap, time::Duration};

use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};
use strum::{Display, IntoStaticStr};

use crate::{
    agent::AgentSession,
    cost::AnalyticsSnapshot,
    vcs::{
        pr::PrMeta,
        repository::{GitRepositoryMeta, RemoteRepositoryMeta},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    /// The global unique identifier for the review.
    pub id: MagicTypeId,

    /// A mapping of the unique Agent IDs to their corresponding AgentSession.
    ///
    /// This will be populated as agents join the review.
    pub agent_sessions: HashMap<MagicTypeId, AgentSession>,

    /// The final analytics snapshot for the review.
    ///
    /// This will not be set until the review is [`ReviewStatus::Failed`], [`ReviewStatus::Completed`], or [`ReviewStatus::Cancelled`].
    pub analytics: Option<AnalyticsSnapshot>,

    /// The duration of the review.
    ///
    /// This will not be set until the review is [`ReviewStatus::Failed`], [`ReviewStatus::Completed`], or [`ReviewStatus::Cancelled`].
    pub duration: Option<Duration>,

    /// The status of the review.
    pub status: ReviewStatus,

    /// Additional metadata about the review.
    pub metadata: ReviewMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewMetadata {
    PullRequest(PullRequestReviewMetadata),
    Commit(CommitReviewMetadata),

    /// No metadata is available for this review.
    Plain,
}

#[derive(
    Display, IntoStaticStr, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum ReviewStatus {
    Pending,
    Running,
    Failed,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestReviewMetadata {
    /// The repository of the PR.
    pub repository: RemoteRepositoryMeta,

    /// Metadata about the PR.
    pub meta: PrMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitReviewMetadata {
    /// The repository of the commit.
    pub repository: GitRepositoryMeta,

    /// The commit range being reviewed.
    pub commit_hash: String,
}
