#[cfg(feature = "binary")]
pub mod config;
pub mod eval;
pub mod finding;
pub mod model_capabilities;
pub mod paths;
pub mod pipeline;
pub mod review;

/// Describes which kind of diff to review.
pub enum ReviewMode {
    /// Review a commit range `base..head`.
    Commits { base: String, head: String },

    /// Review the current working tree (unstaged + staged).
    Working,
}
