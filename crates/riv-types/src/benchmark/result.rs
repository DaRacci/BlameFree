use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

use crate::{benchmark::golden::GoldenComment, finding::Finding};
#[cfg(feature = "seaorm-storage")]
use crate::{
    benchmark::golden::{GoldenCommentColumn, GoldenCommentEntity},
    finding::{FindingColumn, FindingEntity},
};

/// Result of evaluating a benchmark PR.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(riv_macros::EntityModel),
    sea_orm(table_name = "pr_results")
)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrResult {
    /// The [`crate::review::Review::id`] of this PR result.
    pub id: MagicTypeId,

    /// Optional FK back to the [`Benchmark`] that produced this result.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(nullable))]
    pub benchmark_id: Option<MagicTypeId>,

    /// Golden comments for this PR.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(has_many, entity = "GoldenCommentEntity")
    )]
    pub golden_comments: Vec<GoldenComment>,

    /// Findings produced by agent review.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(has_many, entity = "FindingEntity")
    )]
    pub findings: Vec<Finding>,
}
