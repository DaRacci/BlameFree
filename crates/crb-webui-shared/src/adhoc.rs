use serde::{Deserialize, Serialize};

/// Response from POST /api/adhoc/review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocReviewResponse {
    pub run_id: String,
    pub pr_title: String,
    pub status: String,
}

/// Summary of an ad-hoc review run (for the list endpoint).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocRunSummary {
    pub id: String,
    pub pr_url: String,
    pub pr_title: String,
    pub status: String,
    pub created_at: String,
    pub model: String,
    pub roles: Vec<String>,
    pub findings_count: usize,
    pub total_cost: f64,
}

/// A PR from the GitHub API (returned by GET /api/adhoc/prs/:owner/:repo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubPrListItem {
    pub number: u32,
    pub title: String,
    pub html_url: String,
}
