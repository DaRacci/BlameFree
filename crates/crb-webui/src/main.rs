//! Web UI Dashboard for the code review benchmark harness.
//!
//! Provides a browser-based GUI with:
//! - Past run history with metrics
//! - Live agent monitoring via SSE
//! - Benchmark launcher
//! - Per-PR result viewer

use std::path::{Path, PathBuf};
use std::fs;
use std::fs::OpenOptions;

use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

mod api;
mod auth;
mod config;
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

    /// Write logs to this file in addition to stderr.
    #[arg(long, env = "LOG_FILE")]
    pub log_file: Option<PathBuf>,

    /// Path to web UI config file (overrides env/search path).
    #[arg(long)]
    pub config: Option<PathBuf>,
}

/// Auto-detect a writable log file path when `--log-file` is not provided.
///
/// Tries candidates in order, silently skipping paths that can't be created.
fn resolve_log_path(custom: Option<PathBuf>) -> PathBuf {
    if let Some(path) = custom {
        return path;
    }

    let candidates = [
        Path::new("/var/log/crb/webui.log"),
        Path::new("/tmp/crb-webui.log"),
        Path::new("./output/server.log"),
    ];

    for candidate in &candidates {
        if let Some(parent) = candidate.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if OpenOptions::new()
            .create(true)
            .append(true)
            .open(candidate)
            .is_ok()
        {
            return candidate.to_path_buf();
        }
    }

    // Absolute last resort (should always work since ./output/server.log is the last candidate)
    Path::new("./output/server.log").to_path_buf()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Install rustls crypto provider BEFORE any TLS-using code runs.
    // Required by octocrab (hyper-rustls) and reqwest (rustls-tls).
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls ring crypto provider");

    let args = CliArgs::parse();

    // Load .env file before setting up tracing so env-based filter works
    let dotenv_result = dotenvy::dotenv();
    match &dotenv_result {
        Ok(path) => println!("Loaded .env from: {}", path.display()),
        Err(e) => println!("No .env file loaded: {e}"),
    }

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

    // Resolve the log file path: use --log-file if provided, otherwise auto-detect
    let log_path = resolve_log_path(args.log_file.clone());

    // Always set up file + stderr tracing layers using the resolved path
    let log_path_for_tracing = log_path.clone();
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(move || {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path_for_tracing)
                .expect("failed to open log file")
        })
        .with_ansi(false);
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

    tracing::info!(
        "Loaded .env from: {}",
        dotenv_result.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|_| "none".to_string())
    );

    if cfg!(feature = "reduce-diff") {
        tracing::info!("reduce-diff: enabled (-U1 context + metadata stripping)");
    } else {
        tracing::info!("reduce-diff: disabled (full diff)");
    }

    tracing::info!(
        "Starting crb-webui on port {} (output={}, datasets={})",
        args.port,
        args.output_dir.display(),
        args.dataset_dir.display()
    );

    // Load web UI config using Linux standard search path
    let webui_config = config::load_config(args.config.as_deref());
    if webui_config.oauth.is_some() {
        tracing::info!(
            "OAuth is configured (provider={})",
            webui_config.oauth.as_ref().unwrap().provider
        );
    }

    // GitHub API client via octocrab (authenticated with GITHUB_TOKEN env var)
    let octocrab = match std::env::var("GITHUB_TOKEN") {
        Ok(token) => {
            tracing::info!("GITHUB_TOKEN found — octocrab will use it for authenticated requests");
            octocrab::Octocrab::builder()
                .personal_token(token)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build octocrab client: {e}"))?
        }
        Err(_) => {
            tracing::warn!("GITHUB_TOKEN not set — GitHub API rate limits will be low (60 req/hr)");
            octocrab::Octocrab::default()
        }
    };

    // Create session store for OAuth (used regardless of whether OAuth is enabled)
    let session_store = crate::auth::new_session_store();

    let app_state = server::AppState::new(
        args.output_dir,
        args.dataset_dir,
        args.static_dir,
        args.models,
        args.benchmark_dir,
        webui_config,
        octocrab,
        session_store,
        log_path,
    );

    server::start(app_state, args.port).await
}
