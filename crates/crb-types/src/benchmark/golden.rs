use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

#[cfg(feature = "seaorm-storage")]
use crate::benchmark::result::PrResultEntity;
use crate::severity::Severity;

/// A single entry from a golden-comments dataset, representing one PRs expected review findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenCommentEntry {
    /// The PR title.
    pub pr_title: String,

    /// URL to the PR.
    pub url: String,

    /// The list of golden comments for this PR.
    pub comments: Vec<GoldenComment>,
}

/// A single golden comment for a PR.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(crb_macros::EntityModel),
    sea_orm(table_name = "golden_comments")
)]
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct GoldenComment {
    /// Surrogate primary key, auto-incremented by the DB.
    ///
    /// This is only used when storing the golden comments in a database, and is not part of the golden comments dataset itself.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(primary_key, auto_increment = true)
    )]
    pub id: Option<MagicTypeId>,

    /// FK back to the parent [`PrResult`].
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(
            belongs_to,
            entity = "PrResultEntity",
            from = "pr_result_id",
            to = "id",
            on_delete = "Cascade"
        )
    )]
    pub pr_result_id: MagicTypeId,

    /// The expected comment text.
    pub comment: String,

    /// The expected severity of the comment.
    pub severity: Severity,
}
