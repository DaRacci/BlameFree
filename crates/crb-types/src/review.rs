use std::collections::HashMap;
use std::time::Duration;

use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};
use strum::{Display, IntoStaticStr};

use crate::agent::AgentSession;
use crate::cost::AnalyticsSnapshot;
use crate::vcs::pr::PrMeta;
use crate::vcs::repository::{GitRepositoryMeta, RemoteRepositoryMeta};

/// Represents a single LLM review session.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(crb_macros::EntityModel),
    sea_orm(table_name = "reviews")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    /// The global unique identifier for the review.
    pub id: MagicTypeId,

    /// A mapping of the unique Agent IDs to their corresponding AgentSession.
    ///
    /// This will be populated as agents join the review.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(ignore))]
    pub agent_sessions: HashMap<MagicTypeId, AgentSession>,

    /// The final analytics snapshot for the review.
    ///
    /// This will not be set until the review is [`ReviewStatus::Failed`],
    /// [`ReviewStatus::Completed`], or [`ReviewStatus::Cancelled`].
    #[cfg_attr(feature = "seaorm-storage", sea_orm(ignore))]
    pub analytics: Option<AnalyticsSnapshot>,

    /// The duration of the review.
    ///
    /// This will not be set until the review is [`ReviewStatus::Failed`],
    /// [`ReviewStatus::Completed`], or [`ReviewStatus::Cancelled`].
    #[cfg_attr(feature = "seaorm-storage", sea_orm(ignore))]
    pub duration: Option<Duration>,

    /// The status of the review.
    pub status: ReviewStatus,

    /// Additional metadata about the review.
    ///
    /// A domain object — flattened to nullable columns in the DB by the
    /// SqliteStore (Phase 2).
    #[cfg_attr(feature = "seaorm-storage", sea_orm(ignore))]
    pub metadata: ReviewMetadata,
}

/// Discriminated union of review context types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewMetadata {
    PullRequest(PullRequestReviewMetadata),
    Commit(CommitReviewMetadata),

    /// No metadata is available for this review.
    Plain,
}

/// The lifecycle status of a review.
///
/// When the `seaorm-storage` feature is enabled, this derives
/// `DeriveActiveEnum` so the generated `Model` can use it directly
/// as a column type instead of converting to `String`.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(sea_orm::EnumIter, sea_orm::DeriveActiveEnum),
    sea_orm(rs_type = "String", db_type = "Text")
)]
#[derive(
    Display, IntoStaticStr, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum ReviewStatus {
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Pending"))]
    Pending,
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Running"))]
    Running,
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Failed"))]
    Failed,
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Completed"))]
    Completed,
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Cancelled"))]
    Cancelled,
}

/// Metadata for a pull request review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestReviewMetadata {
    /// The repository of the PR.
    pub repository: RemoteRepositoryMeta,

    /// Metadata about the PR.
    pub meta: PrMeta,
}

/// Metadata for a commit review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitReviewMetadata {
    /// The repository of the commit.
    pub repository: GitRepositoryMeta,

    /// The commit range being reviewed.
    pub commit_hash: String,
}
