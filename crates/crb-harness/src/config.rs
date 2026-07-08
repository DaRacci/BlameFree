#[cfg(feature = "binary")]
use clap::Parser;
use std::path::PathBuf;

/// Arguments for the `review_diff` function.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "binary", derive(Parser))]
pub struct ReviewArgs {
    /// Commit range to review (format: base..head).
    #[cfg_attr(feature = "binary", arg(long))]
    pub commits: Option<String>,

    /// Review working tree changes.
    #[cfg_attr(feature = "binary", arg(long, conflicts_with = "commits"))]
    pub working: bool,

    /// Path to the git repository.
    #[cfg_attr(feature = "binary", arg(long, default_value = "."))]
    pub path: PathBuf,

    /// Model to use for agent reviews.
    #[cfg_attr(
        feature = "binary",
        arg(long, env = "MODEL", default_value = "deepseek/deepseek-v4-pro")
    )]
    pub model: String,
}
