//! API route handlers for the web UI dashboard.

pub mod adhoc;
pub mod admin;
pub mod config;
pub mod live;
pub mod runs;

pub use adhoc::{
    get_adhoc_run, list_adhoc_runs, list_repo_prs, start_adhoc_review, AdhocReviewRequest,
};
pub use adhoc::{AdhocReviewResponse, AdhocRunSummary, GithubPrListItem};
pub use admin::{get_logs, get_logs_stream};
pub use config::{get_config, list_dataset_prs, list_datasets, list_reasoning_efforts};
pub use runs::{
    get_agent_log, get_pr_agents, get_pr_detail, get_run, list_logs, list_runs, replay_status,
    start_replay, start_run, BenchmarkConfig,
};

// Re-export live handler with different name to avoid conflicts
pub use live::live_stream;
