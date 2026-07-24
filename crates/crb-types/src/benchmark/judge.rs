use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(feature = "seaorm-storage")]
use crate::finding::FindingEntity;
use crate::{benchmark::golden::GoldenComment, finding::Finding};

/// The structured verdict returned by the judge LLM.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(crb_macros::EntityModel),
    sea_orm(table_name = "judge_verdicts")
)]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JudgeVerdict {
    /// Surrogate primary key
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(primary_key, auto_increment = true)
    )]
    pub id: Option<i32>,

    /// FK back to the parent [`Finding`].
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(
            belongs_to,
            entity = "FindingEntity",
            from = "finding_id",
            to = "id",
            on_delete = "Cascade"
        )
    )]
    pub finding_id: Option<i32>,

    /// Brief explanation of why the judge determined a match or no match.
    #[serde(default)]
    pub reasoning: String,

    /// Whether the candidate finding matches the golden comment.
    #[serde(default, rename = "match")]
    pub match_: bool,

    /// Confidence level for this judgment
    #[serde(default)]
    pub confidence: f64,
}

impl JudgeVerdict {
    pub fn new(reasoning: String, match_: bool, confidence: f64) -> Self {
        Self {
            id: None,
            finding_id: None,
            reasoning,
            match_,
            confidence,
        }
    }
}

/// Contains a list of the findings and verdicts
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct JudgedFindings {
    pub findings: Vec<(Finding, JudgeVerdict)>,
    pub missed_comments: Vec<GoldenComment>,
}
