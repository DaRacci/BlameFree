use serde::{Deserialize, Serialize};

use crate::{
    benchmark::{golden::GoldenComment, judge::JudgeVerdict, metrics::Metrics},
    cost::AnalyticsSnapshot,
    finding::Finding,
    review::Review,
    vcs::{pr::PrMeta, repository::RepositoryMeta},
};

/// Result of evaluating a benchmark PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResult {
    /// The repository of the PR.
    pub repository: Option<RepositoryMeta>,

    /// Metadata about the PR.
    pub meta: PrMeta,

    /// Golden comments for this PR.
    pub golden_comments: Vec<GoldenComment>,

    /// Evaluation metrics.
    pub metrics: Metrics,

    /// Findings and their corresponding verdicts.
    pub findings_with_verdicts: Vec<JudgedFinding>,

    /// The review that was generated for this PR.
    pub review: Review,

    /// Cost tracking data for this PR evaluation.
    #[deprecated = "Use review.analytics instead"]
    pub cost: AnalyticsSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgedFinding {
    pub finding: Finding,
    pub verdict: JudgeVerdict,
}
