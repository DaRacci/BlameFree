use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

use crate::{
    benchmark::{golden::GoldenComment, judge::JudgeVerdict, metrics::Metrics},
    cost::AnalyticsSnapshot,
    finding::Finding,
};

/// Result of evaluating a benchmark PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResult {
    /// The [`crate::review::Review::id`] of this PR result.
    pub id: MagicTypeId,

    /// Golden comments for this PR.
    pub golden_comments: Vec<GoldenComment>,

    /// Evaluation metrics.
    pub metrics: Metrics,

    /// Findings and their corresponding verdicts.
    pub findings_with_verdicts: Vec<JudgedFinding>,

    /// Cost tracking data for this PR evaluation.
    #[deprecated = "Use review.analytics instead"]
    pub cost: AnalyticsSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgedFinding {
    pub finding: Finding,
    pub verdict: JudgeVerdict,
}
