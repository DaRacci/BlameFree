use std::path::PathBuf;

use clap::Args;
use crb_agents::AgentEntry;

/// Arguments for the review subcommand.
///
/// Resolves role abbreviations to typed `AgentEntry` references via `PromptLibrary`,
/// avoiding string-based role identifiers.
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

    /// Comma-separated agent role abbreviations.
    #[arg(long, env = "ROLES", default_value = "SA,CL,AR,SEC")]
    pub roles: String,

    /// Maximum findings per agent.
    #[arg(long, env = "MAX_FINDINGS", default_value_t = 20)]
    pub max_findings: usize,

    /// Cache directory.
    #[arg(long, env = "CACHE_DIR", default_value = "cache")]
    pub cache_dir: PathBuf,
}

impl ReviewArgs {
    /// Resolve role abbreviations to `&'static AgentEntry` references via `PromptLibrary`.
    ///
    /// Uses `PromptLibrary::get_instance().config(abbrev)` to look up each
    /// comma-separated abbreviation, returning only valid known agent entries.
    pub fn resolve_agents(&self) -> Vec<&'static AgentEntry> {
        let library = crb_agents::prompts::PromptLibrary::get_instance();
        self.roles
            .split(',')
            .filter_map(|abbrev| {
                let abbrev = abbrev.trim();
                if abbrev.is_empty() {
                    return None;
                }
                library.config(abbrev)
            })
            .collect()
    }
}
