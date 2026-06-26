use clap::Parser;

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

    /// Directory for evaluation output (JSON per-PR + summary CSV).
    #[arg(long, env = "OUTPUT_DIR", short = 'o', default_value = "output")]
    pub output_dir: String,

    /// Model to use for agent reviews (e.g. gpt-4o, claude-sonnet-4-20250514).
    #[arg(long, env = "MODEL", default_value = "gpt-4o")]
    pub model: String,

    /// Model to use for the LLM judge (e.g. gpt-4o-mini).
    #[arg(long, env = "JUDGE_MODEL", default_value = "gpt-4o-mini")]
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
}
