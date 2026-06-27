//! Web UI Dashboard for the code review benchmark harness.
//!
//! Provides a browser-based GUI with:
//! - Past run history with metrics
//! - Live agent monitoring via SSE
//! - Benchmark launcher
//! - Per-PR result viewer

use std::path::PathBuf;

use clap::Parser;

mod api;
mod converter;
mod events;
mod harness;
mod server;

/// CLI arguments for the web UI dashboard server.
#[derive(Debug, Parser)]
#[command(name = "crb-webui", about = "Web UI dashboard for review-harness")]
pub struct CliArgs {
    /// Port to bind the HTTP server.
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Directory containing benchmark output (per-PR JSON files).
    #[arg(long, env = "OUTPUT_DIR", default_value = "output")]
    pub output_dir: PathBuf,

    /// Path to datasets directory.
    #[arg(long, env = "DATASET_DIR", default_value = "datasets")]
    pub dataset_dir: PathBuf,

    /// Path to the `crb-harness` binary.
    #[arg(
        long,
        env = "HARNESS_PATH",
        default_value = "../target/debug/crb-harness"
    )]
    pub harness_path: PathBuf,

    /// Directory of the static frontend files to serve.
    #[arg(long, default_value = "crates/crb-webui/frontend/dist")]
    pub static_dir: PathBuf,

    /// Comma-separated list of available models.
    #[arg(
        long,
        default_value = "deepseek/deepseek-v4-flash,deepseek/deepseek-v4-pro"
    )]
    pub models: String,

    /// Path to the code-review-benchmark directory (must contain offline/).
    #[arg(long, env = "BENCHMARK_DIR")]
    pub benchmark_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = CliArgs::parse();

    tracing::info!(
        "Starting crb-webui on port {} (output={}, datasets={})",
        args.port,
        args.output_dir.display(),
        args.dataset_dir.display()
    );

    let app_state = server::AppState::new(
        args.output_dir,
        args.dataset_dir,
        args.harness_path,
        args.static_dir,
        args.models,
        args.benchmark_dir,
    );

    server::start(app_state, args.port).await
}
