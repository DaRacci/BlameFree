#[cfg(feature = "binary")]
pub mod config;
pub mod eval;
pub mod finding;
pub mod model_capabilities;
pub mod paths;
pub mod pipeline;
pub mod review;

/// CLI review id prefix, distinct from the webui `review`/`benchmark` prefixes.
pub const CLI_REVIEW_ID_PREFIX: &str = "riv-cli";

/// Generate a unique id for a CLI-driven review.
#[cfg(feature = "binary")]
pub fn cli_review_id() -> mti::prelude::MagicTypeId {
    use mti::prelude::{MagicTypeIdExt, V7};
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{CLI_REVIEW_ID_PREFIX}-{stamp}").create_type_id::<V7>()
}

/// Describes which kind of diff to review.
pub enum ReviewMode {
    /// Review a commit range `base..head`.
    Commits { base: String, head: String },

    /// Review the current working tree (unstaged + staged).
    Working,
}
