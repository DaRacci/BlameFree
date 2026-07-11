use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::{env, fs};

use anyhow::{Result, anyhow};
use clap::Parser;
use octocrab::Octocrab;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod api;
mod auth;
mod config;
mod harness;
mod server;
mod static_assets;

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
    /// If not set, frontend assets embedded at build time are used.
    #[arg(long)]
    pub static_dir: Option<PathBuf>,

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

static CANDIDATES: LazyLock<Vec<&'static Path>> = LazyLock::new(|| {
    vec![
        Path::new("/var/log/crb/webui.log"),
        Path::new("/tmp/crb-webui.log"),
        Path::new("./output/server.log"),
    ]
});

/// Auto-detect a writable log file path when `--log-file` is not provided.
///
/// Tries candidates in order, silently skipping paths that can't be created.
fn resolve_log_path(custom: Option<PathBuf>) -> PathBuf {
    if let Some(path) = custom {
        return path;
    }

    for candidate in CANDIDATES.iter() {
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

    Path::new("./output/server.log").to_path_buf()
}

#[tokio::main]
async fn main() -> Result<()> {
    // Required by octocrab (hyper-rustls) and reqwest (rustls-tls).
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls ring crypto provider");

    let args = CliArgs::parse();

    let dotenv_result = dotenvy::dotenv();
    match &dotenv_result {
        Ok(path) => println!("Loaded .env from: {}", path.display()),
        Err(e) => println!("No .env file loaded: {e}"),
    }

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

    let log_path = resolve_log_path(args.log_file.clone());
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
    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

    info!(
        "Loaded .env from: {}",
        dotenv_result
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "none".to_string())
    );

    if cfg!(feature = "reduce-diff") {
        info!("reduce-diff: enabled (-U1 context + metadata stripping)");
    } else {
        info!("reduce-diff: disabled (full diff)");
    }

    info!(
        "Starting crb-webui on port {} (output={}, datasets={})",
        args.port,
        args.output_dir.display(),
        args.dataset_dir.display()
    );

    let webui_config = config::load_config(args.config.as_deref());
    if webui_config.oauth.is_some() {
        info!(
            "OAuth is configured (provider={})",
            webui_config.oauth.as_ref().unwrap().provider
        );
    }

    let octocrab = match env::var("GITHUB_TOKEN") {
        Ok(token) => {
            info!("GITHUB_TOKEN found — octocrab will use it for authenticated requests");
            Octocrab::builder()
                .personal_token(token)
                .build()
                .map_err(|e| anyhow!("Failed to build octocrab client: {e}"))?
        }
        Err(_) => {
            warn!("GITHUB_TOKEN not set — GitHub API rate limits will be low (60 req/hr)");
            Octocrab::default()
        }
    };

    let app_state = server::AppState::new(
        args.output_dir,
        args.dataset_dir,
        args.static_dir,
        args.models,
        args.benchmark_dir,
        webui_config,
        octocrab,
        crate::auth::new_session_store(),
        log_path,
    );

    crb_harness::model_capabilities::warm_model_cache().await;

    server::start(app_state, args.port).await
}
