use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenComment {
    /// The expected comment text
    pub comment: String,

    /// The expected severity of the comment
    pub severity: Severity,
}
