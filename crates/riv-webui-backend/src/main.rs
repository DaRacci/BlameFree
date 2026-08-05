use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::LazyLock;
use std::{env, fs};

use anyhow::{Result, anyhow};
use clap::Parser;
use leptos::config::get_configuration;
use octocrab::Octocrab;
use riv_agents::prompts::PromptLibrary;
use riv_benchmark as _;
use riv_stor::store::sqlite::SqliteStore;
use tracing::{info, warn};
use tracing_subscriber::util::SubscriberInitExt;

mod auth;
mod config;
mod routes;
mod server;
mod services;

/// CLI arguments for the web UI dashboard server.
#[derive(Debug, Parser)]
#[command(name = "riv-webui", about = "Web UI dashboard for review-harness")]
pub struct CliArgs {
    /// Port to bind the HTTP server.
    #[arg(long)]
    pub port: Option<u16>,

    /// Path to datasets directory.
    #[arg(long, env = "DATASET_DIR", default_value = "datasets")]
    pub dataset_dir: PathBuf,

    /// Path to the code-review-benchmark directory (must contain offline/).
    #[arg(long, env = "BENCHMARK_DIR")]
    pub benchmark_dir: Option<PathBuf>,

    /// Write logs to this file in addition to stderr.
    #[arg(long, env = "LOG_FILE")]
    pub log_file: Option<PathBuf>,

    /// Path to web UI config file
    #[arg(long)]
    pub config: Option<PathBuf>,
}

/// Auto-detect a writable log file path when `--log-file` is not provided.
///
/// Tries candidates in order, silently skipping paths that can't be created.
fn resolve_log_path(custom: Option<&Path>) -> PathBuf {
    static CANDIDATES: LazyLock<Vec<&'static Path>> = LazyLock::new(|| {
        vec![
            Path::new("/var/log/riv/webui.log"),
            Path::new("/tmp/riv-webui.log"),
            Path::new("./server.log"),
        ]
    });

    if let Some(path) = custom {
        return path.to_path_buf();
    }

    for candidate in CANDIDATES.iter() {
        if let Some(parent) = candidate.parent() {
            let _ = fs::create_dir_all(parent); // Ignore — best-effort log directory setup
        }

        if std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(candidate)
            .is_ok()
        {
            return candidate.to_path_buf();
        }
    }

    Path::new("./server.log").to_path_buf()
}

#[tokio::main]
async fn main() -> Result<()> {
    // Required by octocrab (hyper-rustls) and reqwest (rustls-tls).
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls ring crypto provider");

    let args = CliArgs::parse();
    riv_shared::init_dotenv();
    riv_shared::init_logging(Some(resolve_log_path(args.log_file.as_deref()))).try_init()?;

    PromptLibrary::new().map_err(|e| anyhow!("Failed to initialize prompt library: {e}"))?;

    let mut webui_config = config::load_config(args.config.as_deref());
    if webui_config.oauth.is_some() {
        info!(
            "OAuth is configured (provider={})",
            webui_config.oauth.as_ref().unwrap().provider
        );
    }

    if let Some(port) = args.port {
        webui_config.server.port = port;
    }
    webui_config.server.dataset_dir = args.dataset_dir;
    webui_config.server.benchmark_dir = args.benchmark_dir;

    info!("Starting riv-webui on port {}", webui_config.server.port);

    let octocrab = match env::var("GITHUB_TOKEN") {
        Ok(token) => {
            info!("GITHUB_TOKEN found, octocrab will use it for authenticated requests");
            Octocrab::builder()
                .personal_token(token)
                .build()
                .map_err(|e| anyhow!("Failed to build octocrab client: {e}"))?
        }
        Err(_) => {
            warn!("GITHUB_TOKEN not set, GitHub API rate limits will be low (60 req/hr)");
            Octocrab::default()
        }
    };

    // Initialize the store
    let store_path = webui_config.server.store_dir.to_string_lossy().to_string();
    let store = Arc::new(
        SqliteStore::new(&store_path)
            .await
            .map_err(|e| anyhow!("DB init: {e}"))?,
    );

    let conf = get_configuration(None).expect("Failed to read leptos configuration");
    let leptos_options = conf.leptos_options;

    let app_state = server::AppState::<SqliteStore>::new(
        leptos_options,
        webui_config,
        octocrab,
        crate::auth::new_session_store(),
        resolve_log_path(args.log_file.as_deref()),
        store,
    );

    riv_harness::model_capabilities::warm_model_cache().await;

    server::start(app_state).await
}
