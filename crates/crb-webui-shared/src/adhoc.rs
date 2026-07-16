use serde::{Deserialize, Serialize};

/// POST /api/adhoc/review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocReviewResponse {
    /// The run ID for the ad-hoc review.
    pub run_id: String,

    /// PR title for the ad-hoc review.
    pub pr_title: String,

    // TODO: Convert to enum type.
    /// Status of the review (e.g. "running", "completed").
    pub status: String,
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
    pub status: String,

    // TODO: Convert to time type.
    /// ISO-8601 timestamp of when the run was created.
    pub created_at: String,

    /// Model used for the review.
    pub model: String,

    /// Reviewer roles assigned for this run (abbreviation strings).
    /// Now aligned to `Vec<String>` across all API boundaries (see B3/B8/B10).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adhoc_review_response_serde_roundtrip() {
        let orig = AdhocReviewResponse {
            run_id: "run-123".into(),
            pr_title: "Fix the thing".into(),
            status: "completed".into(),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: AdhocReviewResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_adhoc_run_summary_serde_roundtrip() {
        let orig = AdhocRunSummary {
            id: "run-456".into(),
            pr_url: "https://github.com/owner/repo/pull/42".into(),
            pr_title: "Add new feature".into(),
            status: "running".into(),
            created_at: "2024-01-15T10:30:00Z".into(),
            model: "gpt-4o".into(),
            roles: vec!["FE".into(), "BE".into()],
            findings_count: 7,
            total_cost: 0.45,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: AdhocRunSummary = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_adhoc_run_summary_zero_cost() {
        let orig = AdhocRunSummary {
            id: "run-789".into(),
            pr_url: "https://github.com/owner/repo/pull/1".into(),
            pr_title: "Minor fix".into(),
            status: "pending".into(),
            created_at: "2024-02-01T00:00:00Z".into(),
            model: "claude-3.5".into(),
            roles: vec![],
            findings_count: 0,
            total_cost: 0.0,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: AdhocRunSummary = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_github_pr_list_item_serde_roundtrip() {
        let orig = GithubPrListItem {
            number: 42,
            title: "Fix the bug".into(),
            html_url: "https://github.com/owner/repo/pull/42".into(),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: GithubPrListItem = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_github_pr_list_item_minimal_fields() {
        let json = r#"{"number":1,"title":"Fix","html_url":"https://github.com/a/b/pull/1"}"#;
        let item: GithubPrListItem = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(item);
    }
}
