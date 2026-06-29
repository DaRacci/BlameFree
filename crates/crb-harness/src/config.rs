use clap::Parser;
use std::path::PathBuf;

/// CLI arguments for the code review benchmark harness.
#[derive(Debug, Clone, Parser)]
#[command(name = "crb-harness", about = "Code review benchmark harness")]
pub struct CliArgs {
    /// Directory containing golden comment datasets.
    #[arg(long, env = "DATASET_DIR", default_value = "datasets/golden_comments")]
    pub dataset_dir: String,

    /// Directory containing pre-scaffolded repos or diff files.
    #[arg(long, env = "REPOS_DIR", default_value = "repos")]
    pub repos_dir: String,

    /// Directory containing scaffolded repos (from crb-benchmark scaffold).
    /// When set, agents are placed in the repo CWD for full context.
    #[arg(long, env = "SCAFFOLD_DIR")]
    pub scaffold_dir: Option<PathBuf>,

    /// Directory for evaluation output (JSON per-PR + summary CSV).
    #[arg(long, env = "OUTPUT_DIR", short = 'o', default_value = "output")]
    pub output_dir: String,

    /// Model to use for agent reviews (e.g. gpt-4o, claude-sonnet-4-20250514).
    #[arg(long, env = "MODEL", default_value = "deepseek/deepseek-v4-pro")]
    pub model: String,

    /// Model to use for the LLM judge (e.g. gpt-4o-mini).
    #[arg(
        long,
        env = "JUDGE_MODEL",
        default_value = "deepseek/deepseek-v4-flash"
    )]
    pub judge_model: String,

    /// Maximum number of PRs to evaluate concurrently.
    #[arg(long, env = "CONCURRENCY", default_value_t = 4)]
    pub concurrency: usize,

    /// Dry run: load config and datasets, print stats, then exit without making API calls.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// Resume mode: skip PRs that already have result files in the output directory.
    #[arg(long, default_value_t = false)]
    pub resume: bool,

    /// Skip linter execution (only run LLM agents).
    #[arg(long, default_value_t = false)]
    pub skip_linters: bool,

    /// Only run linters, skip LLM agents entirely.
    #[arg(long, default_value_t = false)]
    pub linters_only: bool,

    /// Skip the multi-agent consensus orchestration (use single-agent evaluation).
    #[arg(long, default_value_t = false)]
    pub skip_consensus: bool,

    /// Path to linters.toml configuration file.
    #[arg(long, env = "LINTERS_CONFIG", default_value = "linters.toml")]
    pub linters_config: String,

    /// Path to MCP server configuration TOML file.
    #[arg(long, default_value = "mcp_servers.toml")]
    pub mcp_config: PathBuf,

    /// Path to rules directory (e.g., .crb/rules/).
    #[arg(long, default_value = ".crb/rules/")]
    pub rules_dir: PathBuf,

    /// Skip rule loading entirely.
    #[arg(long, default_value_t = false)]
    pub skip_rules: bool,

    /// Path to prompts directory (e.g., prompts/experiments/EXP-013).
    /// Defaults to "prompts/builtin" which contains the built-in defaults.
    /// Custom directories override the built-in prompts with file-based ones.
    #[arg(long, env = "PROMPTS_DIR", default_value = "prompts/builtin")]
    pub prompts_dir: PathBuf,

    /// Validate mode: load baseline JSON and compare metrics against output.
    #[arg(long, default_value_t = false)]
    pub validate: bool,

    /// CI mode: run full pipeline (scaffold → evaluate → validate → report) with exit code.
    #[arg(long, default_value_t = false)]
    pub ci: bool,

    /// Skip scaffolding and use pre-extracted diffs.
    #[arg(long, default_value_t = false)]
    pub cached_diffs: bool,

    /// Comma-separated list of agent roles to use (default: SA,CL,AR,SEC).
    #[arg(long, env = "ROLES", default_value = "SA,CL,AR,SEC")]
    pub roles: String,

    /// Maximum number of findings per agent (default: 20).
    #[arg(long, env = "MAX_FINDINGS", default_value_t = 20)]
    pub max_findings: usize,

    /// Only evaluate PRs matching these repo or PR number patterns (comma-separated).
    /// Useful for smoke tests. Example: --pr-filter "discourse-7,calcom-11059"
    #[arg(long)]
    pub pr_filter: Option<String>,

    /// Directory to cache all LLM interactions for debugging/replay.
    /// Creates per-PR subdirectories with prompts, responses, and judge calls.
    #[arg(long, env = "CACHE_DIR")]
    pub cache_dir: Option<PathBuf>,

    /// Directory containing pre-recorded LLM interactions for deterministic replay.
    #[arg(long, env = "REPLAY_DIR")]
    pub replay_dir: Option<PathBuf>,

    /// Enable live interactive dashboard (TUI).
    /// When set, renders a 4-pane agent view with progress, cost, and summaries.
    #[arg(long, default_value_t = false)]
    pub dashboard: bool,

    /// When set, output JSON dashboard events to stdout (one per line).
    #[arg(long, default_value_t = false)]
    pub dashboard_events: bool,
}
