use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

#[cfg(feature = "seaorm-storage")]
use crate::{
    benchmark::golden::{GoldenCommentActiveModel, GoldenCommentEntity, GoldenCommentModel},
    benchmark::judge::JudgeVerdictModel,
    finding::{FindingActiveModel, FindingEntity, FindingModel},
};
use crate::{
    benchmark::{golden::GoldenComment, judge::JudgeVerdict},
    finding::Finding,
};

/// Result of evaluating a benchmark PR.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(crb_macros::EntityModel),
    sea_orm(table_name = "pr_results")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Findings and their corresponding verdicts.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(has_many, entity = "FindingEntity", tuple = "JudgeVerdictEntity")
    )]
    pub findings_with_verdicts: Vec<(Finding, JudgeVerdict)>,
}
