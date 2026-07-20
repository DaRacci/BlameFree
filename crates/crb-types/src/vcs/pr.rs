use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "seaorm-storage", derive(crb_macros::FlattenedStruct))]
pub struct PrMeta {
    /// The title of the PR.
    pub title: String,

    /// URL to the PR.
    pub url: String,

    /// PR number.
    pub number: u32,
}
