//! API route handlers for the web UI dashboard.

pub mod config;
pub mod live;
pub mod runs;

pub use config::{get_config, list_datasets};
pub use runs::{get_run, list_runs, start_run, BenchmarkConfig};

// Re-export live handler with different name to avoid conflicts
pub use live::live_stream;
