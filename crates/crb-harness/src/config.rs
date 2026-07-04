use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
pub enum Cli {
    /// Review a git diff (working tree or commit range)
    Review(ReviewArgs),
}

#[derive(Debug, Clone, Parser)]
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

    /// Model to use for agent reviews (e.g. gpt-4o, claude-sonnet-4-20250514, deepseek/deepseek-v4-flash).
    #[arg(long, env = "MODEL", default_value = "deepseek/deepseek-v4-pro")]
    pub model: String,
}
