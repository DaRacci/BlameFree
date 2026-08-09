#[cfg(feature = "backend")]
use {
    anyhow::Result, tracing::level_filters::LevelFilter, tracing_appender::rolling::Rotation,
    tracing_subscriber::fmt::format::FmtSpan,
};

pub mod deduplicate;
pub mod diff;
pub mod filter;
pub mod fs;
pub mod jaccard;
pub mod pattern;
pub mod string;
pub mod url;

pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-flash";
pub const DEFAULT_MODEL_PRO: &str = "deepseek/deepseek-v4-pro";
pub const DEFAULT_MAX_FINDINGS: usize = 20;

pub const LATEST_LOG_FILENAME: &str = "latest.log";

#[cfg(feature = "backend")]
pub fn build_client() -> Result<rig_core::providers::openrouter::client::Client> {
    use anyhow::{Context, anyhow};

    let api_key = std::env::var("OPENROUTER_API_KEY")
        .map_err(|_| anyhow!("OPENROUTER_API_KEY environment variable not set"))?;

    rig_core::providers::openrouter::client::Client::builder()
        .api_key(api_key)
        .with_app_categories(&["cli-agent"])
        .with_app_identity("BlameFree", env!("CARGO_PKG_REPOSITORY"))
        .build()
        .context("Failed to build OpenRouter client")
}

#[cfg(feature = "backend")]
pub fn init_dotenv() {
    match dotenvy::dotenv() {
        Ok(path) => eprintln!("Loaded .env file from: {:?}", path.display()),
        Err(e) => eprintln!("No .env file found or failed to load: {}", e),
    }
}

/// Initialize tracing with stderr output and an optional log directory.
///
/// Returns a subscriber ready for `.try_init()?`.
#[cfg(feature = "backend")]
pub fn init_logging(
    log_dir: Option<std::path::PathBuf>,
) -> Box<dyn tracing::Subscriber + Send + Sync> {
    use tracing_subscriber::filter::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt;

    let env_layer = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sea_orm=warn,sqlx=warn"))
        // Suppress rig's LLM telemetry: its streaming spans record the same `gen_ai.*` fields on every chunk,
        // which the fmt layer accumulates, producing enormous log lines.
        .add_directive("rig::agent_chat=off".parse().expect("valid directive"))
        .add_directive("rig::streaming=off".parse().expect("valid directive"))
        .add_directive("rig::completions=off".parse().expect("valid directive"))
        .add_directive("rig::embedding=off".parse().expect("valid directive"))
        .add_directive("rig::transcription=off".parse().expect("valid directive"))
        .add_directive("rig_core=off".parse().expect("valid directive"));

    let span_events = if env_layer.max_level_hint() == Some(LevelFilter::TRACE) {
        FmtSpan::ACTIVE
    } else {
        FmtSpan::NONE
    };

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_span_events(span_events.clone())
        .with_target(false)
        .with_thread_ids(true)
        .with_writer(std::io::stderr);

    let subscriber = tracing_subscriber::registry()
        .with(env_layer)
        .with(stderr_layer);

    let Some(path) = log_dir else {
        return Box::new(subscriber);
    };

    let file_appender = tracing_appender::rolling::Builder::new()
        .latest_symlink(LATEST_LOG_FILENAME)
        .rotation(Rotation::DAILY)
        .filename_prefix("webui")
        .max_log_files(7)
        .build(&path)
        .expect("failed to create log file appender");
    let (appender, guard) = tracing_appender::non_blocking(file_appender);

    // The `WorkerGuard` must outlive the subscriber,
    // leak it for the lifetime of the process since logging is initialized once.
    std::mem::forget(guard);

    Box::new(
        subscriber.with(
            tracing_subscriber::fmt::layer()
                .with_writer(appender)
                .with_span_events(span_events)
                .with_target(false)
                .with_ansi(false)
                .with_thread_ids(false),
        ),
    )
}
