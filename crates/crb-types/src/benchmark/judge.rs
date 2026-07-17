use serde::{Deserialize, Serialize};

/// The structured verdict returned by the judge LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeVerdict {
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
