use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

#[cfg(feature = "seaorm-storage")]
use crate::benchmark::golden::GoldenCommentEntity;
#[cfg(feature = "seaorm-storage")]
use crate::finding::FindingEntity;
use crate::{
    benchmark::{golden::GoldenComment, judge::JudgeVerdict, metrics::Metrics},
    cost::AnalyticsSnapshot,
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

    /// Evaluation metrics.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(ignore))]
    #[deprecated = "Use MetricsProvider from Vec<(Finding, JudgeVerdict)> instead"]
    pub metrics: Metrics,

    /// Findings and their corresponding verdicts.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(has_many, entity = "FindingEntity")
    )]
    pub findings_with_verdicts: Vec<(Finding, JudgeVerdict)>,

    /// Cost tracking data for this PR evaluation.
    #[deprecated = "Use review.analytics instead"]
    #[cfg_attr(feature = "seaorm-storage", sea_orm(ignore))]
    pub cost: AnalyticsSnapshot,
}
