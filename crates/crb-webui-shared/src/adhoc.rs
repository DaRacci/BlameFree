use serde::{Deserialize, Serialize};

use crate::review::RunStatus;

/// POST /api/adhoc/review
///
/// Thin response wrapper returned immediately when an ad-hoc review is started.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated = "Migrate to new crb-types interfaces using [`crb_types::review::Review`]"]
pub struct AdhocReviewResponse {
    /// The run ID for the ad-hoc review.
    pub run_id: String,

    /// PR title for the ad-hoc review.
    pub pr_title: String,

    /// Status of the review
    pub status: RunStatus,
}
