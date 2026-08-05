use riv_shared::{DEFAULT_MAX_FINDINGS, DEFAULT_MODEL};
use std::path::PathBuf;

use clap::Args;

/// Arguments for the review subcommand.
#[derive(Debug, Clone, Args)]
pub struct ReviewArgs {
    /// Commit range to review
    ///
    /// This expects a format of `base..head`,
    /// (e.g., `HEAD~3..HEAD`).
    #[arg(long)]
    pub commits: Option<String>,

    /// Review working tree changes
    #[arg(long, conflicts_with = "commits")]
    pub working: bool,

    /// Path to the git repository
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// Model to use for agent reviews.
    #[arg(long, env = "MODEL", default_value_t = DEFAULT_MODEL.to_string())]
    pub model: String,

    /// Comma-separated agent role abbreviations to use instead of all available agents.
    #[arg(long, env = "ROLES", value_delimiter = ',')]
    pub roles: Option<Vec<String>>,

    /// Maximum findings per agent.
    #[arg(long, env = "MAX_FINDINGS", default_value_t = DEFAULT_MAX_FINDINGS)]
    pub max_findings: usize,

    /// Cache directory.
    #[arg(long, env = "CACHE_DIR", default_value = "cache")]
    pub cache_dir: PathBuf,
}

#[cfg(test)]
#[cfg(feature = "binary")]
mod tests {
    use super::*;

    #[test]
    fn test_review_args_defaults() {
        let args = ReviewArgs {
            commits: None,
            working: false,
            path: PathBuf::from("."),
            model: "deepseek/deepseek-v4-pro".to_string(),
            roles: None,
            max_findings: 20,
            cache_dir: PathBuf::from("cache"),
        };
        assert_eq!(args.path, PathBuf::from("."));
        assert_eq!(args.model, "deepseek/deepseek-v4-pro");
        assert_eq!(args.max_findings, 20);
        assert_eq!(args.cache_dir, PathBuf::from("cache"));
        assert!(args.commits.is_none());
        assert!(!args.working);
        assert!(args.roles.is_none());
    }
}
