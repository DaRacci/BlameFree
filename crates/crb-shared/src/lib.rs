#[cfg(feature = "backend")]
use anyhow::Result;

pub mod deduplicate;
pub mod diff;
pub mod filter;
pub mod fs;
pub mod jaccard;
pub mod pattern;
pub mod url;

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

/// Sanitize a string for use as a filename.
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Strip markdown formatting characters and normalize whitespace.
///
/// Lowercases, removes common markdown sigils (`*`, `_`, `` ` ``, `#`, `[`,`]`),
/// and collapses multiple whitespace into single spaces.
pub fn normalize_text(text: &str) -> String {
    let text = text.to_lowercase();
    let text: String = text
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '[' | ']'))
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    // Collapse multiple spaces into one
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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

#[cfg(feature = "backend")]
pub fn init_logging(
    layers: Vec<tracing_subscriber::fmt::Layer<tracing_subscriber::fmt::format::DefaultFields>>,
) {
    use tracing_subscriber::filter::env::EnvFilter;

    let env_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = tracing_subscriber::registry().with(env_layer);

    let subscriber = layers
        .into_iter()
        .fold(subscriber, |sub, layer| sub.with(layer));

    subscriber.init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_via_utils() {
        assert_eq!(sanitize_filename("hello world"), "hello_world");
        assert_eq!(sanitize_filename("file.name.txt"), "file_name_txt");
        assert_eq!(sanitize_filename("already_ok"), "already_ok");
        assert_eq!(sanitize_filename(""), "");
        assert_eq!(sanitize_filename("a|b<c>d:e"), "a_b_c_d_e");
    }

    #[test]
    fn normalize_strips_markdown() {
        let n = normalize_text(" **CRITICAL**: This is a *test* ");
        assert!(!n.contains('*'));
        assert!(!n.contains('#'));
        assert_eq!(n, "critical: this is a test");
    }
}
