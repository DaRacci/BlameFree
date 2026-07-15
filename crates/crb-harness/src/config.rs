use std::path::PathBuf;

use clap::Args;

/// Arguments for the review subcommand.
///
/// Agent roles are resolved through `PromptLibrary` — either from the
/// `--roles` flag (comma-separated abbreviations) or, when omitted, using
/// every available agent in the loaded prompt library.
#[derive(Debug, Clone, Args)]
pub struct ReviewArgs {
    /// Commit range to review (format: base..head, e.g. "HEAD~3..HEAD")
    #[arg(long)]
    pub commits: Option<String>,

    /// Review working tree changes (unstaged + staged)
    #[arg(long, conflicts_with = "commits")]
    pub working: bool,

    /// Path to the git repository
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// Model to use for agent reviews.
    #[arg(long, env = "MODEL", default_value = "deepseek/deepseek-v4-pro")]
    pub model: String,

    /// Comma-separated agent role abbreviations to use instead of all available agents.
    #[arg(long, env = "ROLES", value_delimiter = ',')]
    pub roles: Option<Vec<String>>,

    /// Maximum findings per agent.
    #[arg(long, env = "MAX_FINDINGS", default_value_t = 20)]
    pub max_findings: usize,

    /// Cache directory.
    #[arg(long, env = "CACHE_DIR", default_value = "cache")]
    pub cache_dir: PathBuf,
}
