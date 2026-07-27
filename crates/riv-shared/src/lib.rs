#[cfg(feature = "backend")]
use anyhow::Result;

pub mod deduplicate;
pub mod diff;
pub mod filter;
pub mod fs;
pub mod jaccard;
pub mod pattern;
pub mod url;

pub mod string;
#[cfg(test)]
pub mod test_utils;

/// Default model for ad-hoc and judge review tasks.
pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-flash";

/// Default model for benchmark/harness reviews (often a larger model).
pub const DEFAULT_MODEL_PRO: &str = "deepseek/deepseek-v4-pro";

pub const OUTPUT_DIR: &str = "output";

pub const OUTPUT_CACHE_DIR: &str = ".cache";

pub fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

pub fn default_model_pro() -> String {
    DEFAULT_MODEL_PRO.to_string()
}

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

/// Initialize tracing with stderr output and an optional log file.
///
/// Returns a subscriber ready for `.try_init()?`.
#[cfg(feature = "backend")]
pub fn init_logging(
    log_file: Option<std::path::PathBuf>,
) -> Box<dyn tracing::Subscriber + Send + Sync> {
    use std::fs::OpenOptions;
    use tracing_subscriber::filter::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt;

    let env_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    let subscriber = tracing_subscriber::registry()
        .with(env_layer)
        .with(stderr_layer);

    if let Some(path) = log_file {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(move || {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .expect("failed to open log file")
            })
            .with_ansi(false);
        Box::new(subscriber.with(file_layer))
    } else {
        Box::new(subscriber)
    }
}
