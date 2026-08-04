use std::time::Duration;

use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};
use strum::{Display, IntoStaticStr};

use crate::cost::AnalyticsSnapshot;

#[cfg(not(feature = "seaorm-storage"))]
use crate::agent::AgentSession;
#[cfg(feature = "seaorm-storage")]
use crate::agent::{AgentSession, AgentSessionColumn, AgentSessionEntity};
use crate::vcs::pr::PrMeta;
use crate::vcs::repository::{GitRepositoryMeta, RemoteRepositoryMeta};

/// Runtime-only review data (analytics + duration) folded into a single
/// zstd-compressed JSON blob stored in the `reviews.analytics` BLOB column.
///
/// `Review.analytics` and `Review.duration` are `#[sea_orm(json)]` fields; the
/// EntityModel macro wraps them into this struct before persisting.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewRuntimeData {
    /// The analytics snapshot for the review.
    pub analytics: AnalyticsSnapshot,

    /// The duration of the review in nanoseconds.
    pub duration_nanos: u64,
}

/// Represents a single LLM review session.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(riv_macros::EntityModel),
    sea_orm(table_name = "reviews")
)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Review {
    /// The global unique identifier for the review.
    pub id: MagicTypeId,

    /// Agent sessions for this review.
    ///
    /// This will be populated as agents join the review.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(has_many, entity = "AgentSessionEntity", child_fk = "review_id")
    )]
    pub agent_sessions: Vec<AgentSession>,

    /// The final analytics snapshot for the review.
    ///
    /// This will not be set until the review is [`ReviewStatus::Failed`],
    /// [`ReviewStatus::Completed`], or [`ReviewStatus::Cancelled`].
    ///
    /// Stored as a single zstd-compressed JSON blob in the `analytics` column
    /// (alongside [`Review::duration`]) via the EntityModel `#[sea_orm(json)]`
    /// attribute. See [`ReviewRuntimeData`].
    #[cfg_attr(feature = "seaorm-storage", sea_orm(json))]
    pub analytics: Option<AnalyticsSnapshot>,

    /// The duration of the review.
    ///
    /// This will not be set until the review is [`ReviewStatus::Failed`],
    /// [`ReviewStatus::Completed`], or [`ReviewStatus::Cancelled`].
    ///
    /// Persisted via the same compressed blob as [`Review::analytics`]; see
    /// [`ReviewRuntimeData`].
    #[cfg_attr(feature = "seaorm-storage", sea_orm(json))]
    pub duration: Option<Duration>,

    /// The status of the review.
    pub status: ReviewStatus,

    /// Additional metadata about the review.
    ///
    /// A domain object — flattened to nullable columns in the DB by the
    /// EntityModel `#[flatten]` attribute.
    #[cfg_attr(
        feature = "seaorm-storage",
        flatten(
            tag = "review_type",
            variants = {
                pull_request => {
                    repository_owner: String,
                    repository_name: String,
                    repository_platform: String,
                    pr_title: String,
                    pr_url: String,
                    pr_number: i32,
                },
                commit => { commit_hash: String },
                plain => {}
            }
        )
    )]
    pub metadata: ReviewMetadata,
}

/// Discriminated union of review context types.
#[cfg_attr(feature = "seaorm-storage", derive(riv_macros::FlattenedEnum))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewMetadata {
    #[cfg_attr(
        feature = "seaorm-storage",
        variant(
            tag = "pull_request",
            fields(
                repository_owner: String,
                repository_name: String,
                repository_platform: String,
                pr_title: String,
                pr_url: String,
                pr_number: i32,
            )
        )
    )]
    PullRequest(PullRequestReviewMetadata),

    #[cfg_attr(
        feature = "seaorm-storage",
        variant(
            tag = "commit",
            fields(
                commit_hash: String,
            )
        )
    )]
    Commit(CommitReviewMetadata),

    /// No metadata is available for this review.
    #[cfg_attr(feature = "seaorm-storage", variant(tag = "plain"))]
    Plain,
}

impl Default for ReviewMetadata {
    fn default() -> Self {
        ReviewMetadata::Plain
    }
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
    Default,
    Display,
    IntoStaticStr,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
)]
pub enum ReviewStatus {
    #[cfg_attr(feature = "seaorm-storage", sea_orm(string_value = "Pending"))]
    #[default]
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
#[cfg_attr(feature = "seaorm-storage", derive(riv_macros::FlattenedStruct))]
pub struct PullRequestReviewMetadata {
    /// The repository of the PR.
    pub repository: RemoteRepositoryMeta,

    /// Metadata about the PR.
    #[cfg_attr(feature = "seaorm-storage", flattened(prefix = "pr_"))]
    pub meta: PrMeta,
}

/// Metadata for a commit review.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "seaorm-storage", derive(riv_macros::FlattenedStruct))]
pub struct CommitReviewMetadata {
    /// The repository of the commit.
    pub repository: GitRepositoryMeta,

    /// The commit range being reviewed.
    pub commit_hash: String,
}
