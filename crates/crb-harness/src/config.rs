#[cfg(feature = "binary")]
use clap::Parser;
use crb_shared::DEFAULT_MODEL;
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
        arg(long, env = "MODEL", default_value = "default_model")
    )]
    pub model: String,
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

#[cfg(test)]
#[cfg(feature = "binary")]
mod tests {
    use super::*;

    #[test]
    fn review_args_default_path_is_dot() {
        // Simulate `--working` with no path specified
        let args = ReviewArgs::parse_from(["test", "--working"]);
        assert_eq!(args.path, PathBuf::from("."));
        assert!(args.working);
        assert!(args.commits.is_none());
    }

    #[test]
    fn review_args_commit_range() {
        let args = ReviewArgs::parse_from(["test", "--commits", "HEAD~3..HEAD"]);
        assert_eq!(args.commits.as_deref(), Some("HEAD~3..HEAD"));
        assert!(!args.working);
    }

    #[test]
    fn review_args_custom_path() {
        let args = ReviewArgs::parse_from([
            "test",
            "--working",
            "--path",
            "/some/repo",
            "--model",
            "gpt-4o",
        ]);
        assert_eq!(args.path, PathBuf::from("/some/repo"));
        assert_eq!(args.model, "gpt-4o");
    }
}
