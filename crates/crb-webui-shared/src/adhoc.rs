use serde::{Deserialize, Serialize};

use crate::runs::RunStatus;

/// POST /api/adhoc/review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocReviewResponse {
    /// The run ID for the ad-hoc review.
    pub run_id: String,

    /// PR title for the ad-hoc review.
    pub pr_title: String,

    /// Status of the review
    pub status: RunStatus,
}

/// Summary of an ad-hoc review run (for the list endpoint).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocRunSummary {
    /// Unique run identifier.
    pub id: String,

    /// URL to the PR being reviewed.
    pub pr_url: String,

    /// PR title.
    pub pr_title: String,

    /// Status of the run.
    pub status: RunStatus,

    // TODO: Convert to time type.
    /// ISO-8601 timestamp of when the run was created.
    pub created_at: String,

    /// Model used for the review.
    pub model: String,

    /// Reviewer roles assigned for this run
    pub roles: Vec<String>,

    /// Number of findings produced.
    pub findings_count: usize,

    /// Total cost in USD for this run.
    pub total_cost: f64,
}

/// GET /api/adhoc/prs/:owner/:repo
///
/// A PR from the GitHub API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubPrListItem {
    /// GitHub PR number.
    pub number: u32,

    /// PR title.
    pub title: String,

    /// URL to the PR on GitHub.
    pub html_url: String,
}
