use std::collections::HashMap;

use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

use crate::{
    agent::AgentSession,
    cost::AnalyticsSnapshot,
    vcs::{
        pr::PrMeta,
        repository::{GitRepositoryMeta, RemoteRepositoryMeta},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review<M>
where
    M: ReviewMetadata,
{
    pub id: MagicTypeId,

    pub agent_sessions: HashMap<MagicTypeId, AgentSession>,

    pub analytics: AnalyticsSnapshot,

    pub metadata: M,
}

pub trait ReviewMetadata {}

impl ReviewMetadata for () {}

pub struct PullRequestReviewMetadata {
    /// The repository of the PR.
    pub repository: RemoteRepositoryMeta,

    /// Metadata about the PR.
    pub meta: PrMeta,
}

pub struct CommitReviewMetadata {
    /// The repository of the commit.
    pub repository: GitRepositoryMeta,

    /// The commit range being reviewed.
    pub commit_hash: String,
}
