//! API route handlers for the web UI dashboard.

pub mod adhoc;
pub mod config;
pub mod live;
pub mod runs;

pub use adhoc::{
    start_adhoc_review, list_adhoc_runs, get_adhoc_run, list_repo_prs, GithubPrListItem, AdhocReviewRequest, AdhocReviewResponse,
    AdhocRunSummary,
};
pub use config::{get_config, list_datasets, list_dataset_prs};
pub use runs::{
    get_run, list_runs, start_run, BenchmarkConfig,
    list_logs, get_agent_log, get_pr_agents, start_replay, replay_status,
    get_pr_detail,
};

// Re-export live handler with different name to avoid conflicts
pub use live::live_stream;
