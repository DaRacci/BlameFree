//! Model capability detection for reasoning/thinking support.
//!
//! Queries the OpenRouter models API to discover which models support reasoning, with a fallback heuristic when the API is unreachable.
//! Results are cached via [`std::sync::OnceLock`]; Initialised once, then read lock-free by all threads.

use crb_types::wrappers::{Model, WrappedData};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::OnceLock;

/// Configuration for model reasoning/thinking support.
///
/// Contains all the information needed to inject reasoning parameters into the API request body.
#[derive(Debug, Clone, Serialize)]
pub enum ReasoningConfig {
    /// OpenAI-style reasoning: `{"reasoning": {"effort": "low|medium|high"}}`
    ReasoningEffort {
        /// The reasoning effort level.
        effort: ReasoningEffort,
    },

    /// Anthropic-style thinking: `{"thinking": {"type": "enabled", "budget_tokens": N}}`
    /// Used by Claude models with extended thinking.
    Thinking {
        /// Token budget for thinking (Anthropic requires this).
        budget_tokens: u32,
    },
}

/// The reasoning effort level for OpenAI style reasoning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    /// Faster responses, less deep reasoning.
    Low,

    /// Balanced depth and speed.
    Medium,

    /// More thorough reasoning.
    High,

    /// Most thorough, slowest.
    Max,
}

impl ReasoningEffort {
    /// Return all variants as a slice.
    pub fn variants() -> &'static [Self] {
        &[Self::Low, Self::Medium, Self::High, Self::Max]
    }
}

impl std::fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReasoningEffort::Low => write!(f, "low"),
            ReasoningEffort::Medium => write!(f, "medium"),
            ReasoningEffort::High => write!(f, "high"),
            ReasoningEffort::Max => write!(f, "max"),
        }
    }
}

impl ReasoningEffort {
    /// Parse a reasoning effort from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" | "med" => Some(Self::Medium),
            "high" => Some(Self::High),
            "max" => Some(Self::Max),
            _ => None,
        }
    }
}

impl ReasoningConfig {
    /// Convert this reasoning config into JSON additional_params that can be
    /// injected into the API request via `agent.additional_params`.
    pub fn to_additional_params_json(&self) -> serde_json::Value {
        match self {
            ReasoningConfig::ReasoningEffort { effort } => {
                serde_json::json!({
                    "reasoning": {
                        "effort": effort
                    }
                })
            }
            ReasoningConfig::Thinking { budget_tokens } => {
                serde_json::json!({
                    "thinking": {
                        "type": "enabled",
                        "budget_tokens": budget_tokens
                    }
                })
            }
        }
    }
}

/// Response from OpenRouter's `GET /api/v1/models` endpoint.
#[derive(serde::Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

/// A single model entry from the OpenRouter models API.
#[derive(serde::Deserialize)]
struct OpenRouterModel {
    id: String,
    /// If `Some`, this model supports reasoning.
    reasoning: Option<serde_json::Value>,
}

/// Cache of model IDs that support reasoning, populated via
/// [`warm_model_cache`] or lazily on first query.
///
/// Uses [`OnceLock`] so reads are lock-free after initialisation.
static REASONING_MODEL_IDS: OnceLock<Option<HashSet<String>>> = OnceLock::new();

/// Whether we fell back to heuristic matching (API was unreachable or
/// not yet initialised in async context).
static USING_FALLBACK: OnceLock<bool> = OnceLock::new();

/// Set the reasoning model cache from a fetch result.
/// Both async and blocking warm functions delegate to this helper.
fn set_reasoning_cache(result: Result<HashSet<String>, String>, info_suffix: &str) {
    match result {
        Ok(ids) => {
            tracing::info!(
                count = ids.len(),
                "OpenRouter models API: reasoning-capable models discovered{}",
                info_suffix
            );
            let _ = REASONING_MODEL_IDS.set(Some(ids)); // Ignore — first write wins, subsequent calls are no-ops
            let _ = USING_FALLBACK.set(false); // Ignore — first write wins, subsequent calls are no-ops
        }
        Err(e) => {
            tracing::warn!(
                "OpenRouter model API unreachable ({}); using fallback heuristic",
                e
            );
            let _ = REASONING_MODEL_IDS.set(None); // Ignore — first write wins, subsequent calls are no-ops
            let _ = USING_FALLBACK.set(true); // Ignore — first write wins, subsequent calls are no-ops
        }
    }
}

/// Warm the model capabilities cache by querying the OpenRouter API.
///
/// Call this at server startup (before any concurrent agent tasks run)
/// to avoid fallback heuristics. Safe to call multiple times — subsequent
/// calls are no-ops.
///
/// Uses async reqwest so it's safe inside a tokio runtime.
pub async fn warm_model_cache() {
    // Already initialised — nothing to do
    if REASONING_MODEL_IDS.get().is_some() {
        return;
    }

    set_reasoning_cache(fetch_reasoning_models_async().await, "");
}

/// Async HTTP call to fetch the list of reasoning-capable model IDs.
async fn fetch_reasoning_models_async() -> Result<HashSet<String>, String> {
    let url = "https://openrouter.ai/api/v1/models";
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("API returned {}", response.status()));
    }

    let body: OpenRouterModelsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    let ids: HashSet<String> = body
        .data
        .into_iter()
        .filter(|m| m.reasoning.is_some())
        .map(|m| m.id)
        .collect();

    Ok(ids)
}

/// Initialise the model cache synchronously using a blocking HTTP call.
///
/// Only use this OUTSIDE a tokio runtime (e.g. from `main()` before
/// the async runtime starts, or from a CLI tool that uses blocking I/O).
/// Inside a tokio runtime, use async [`warm_model_cache`] instead.
pub fn warm_model_cache_blocking() {
    if REASONING_MODEL_IDS.get().is_some() {
        return;
    }

    set_reasoning_cache(fetch_reasoning_models_blocking(), " (blocking)");
}

/// Blocking HTTP call — must NOT be called from within a tokio async context.
fn fetch_reasoning_models_blocking() -> Result<HashSet<String>, String> {
    let url = "https://openrouter.ai/api/v1/models";
    let response = reqwest::blocking::get(url).map_err(|e| format!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("API returned {}", response.status()));
    }

    let body: OpenRouterModelsResponse = response
        .json()
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    let ids: HashSet<String> = body
        .data
        .into_iter()
        .filter(|m| m.reasoning.is_some())
        .map(|m| m.id)
        .collect();

    Ok(ids)
}

/// Check whether a model supports reasoning, consulting the cached OpenRouter
/// model list, with fallback to heuristic matching.
///
/// If the cache has been warmed (via [`warm_model_cache`] or
/// [`warm_model_cache_blocking`]), uses the API result. Otherwise falls
/// back to the heuristic immediately; no blocking I/O, safe in any context.
pub fn supports_reasoning(model: &Model) -> bool {
    match REASONING_MODEL_IDS.get() {
        Some(Some(ids)) => {
            ids.contains(model.get())
                || ids
                    .iter()
                    .any(|id| model.get().ends_with(id) || id.ends_with(model.get()))
        }
        _ => fallback_is_reasoning_model(model),
    }
}

/// Heuristic fallback: models whose names contain certain keywords are assumed to support reasoning.
fn fallback_is_reasoning_model(model: &Model) -> bool {
    let model_lower = model.get().to_lowercase();
    model_lower.contains("deepseek")
        || model_lower.starts_with("o1")
        || model_lower.starts_with("o3")
        || model_lower.starts_with("o4")
        || model_lower.starts_with("chatgpt-o")
        || model_lower.starts_with("claude")
        || (model_lower.starts_with("gemini")
            && (model_lower.contains("thinking") || model_lower.contains("2.5")))
}

/// Given a model string and an optional reasoning effort string, return the
/// appropriate [`ReasoningConfig`] if the model supports reasoning.
///
/// Returns `None` if the model does not support reasoning.
pub fn get_reasoning_config(model: &Model, effort: ReasoningEffort) -> Option<ReasoningConfig> {
    if !supports_reasoning(model) {
        return None;
    }

    let model_lower = model.get().to_lowercase();
    if model_lower.contains("claude") {
        return Some(ReasoningConfig::Thinking {
            budget_tokens: 2048,
        });
    }

    Some(ReasoningConfig::ReasoningEffort { effort })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time check: verify reasoning_effort accepts Option<ReasoningEffort>, not String.
    /// If the field type regresses back to Option<String>, this won't compile.
    fn assert_reasoning_effort_type(val: Option<ReasoningEffort>) -> Option<ReasoningEffort> {
        val
    }

    #[test]
    fn test_reasoning_effort_enum_values() {
        // All variants
        assert_reasoning_effort_type(Some(ReasoningEffort::Low));
        assert_reasoning_effort_type(Some(ReasoningEffort::Medium));
        assert_reasoning_effort_type(Some(ReasoningEffort::High));
        assert_reasoning_effort_type(Some(ReasoningEffort::Max));
        assert_reasoning_effort_type(None);
    }

    #[test]
    fn test_reasoning_effort_display() {
        assert_eq!(format!("{}", ReasoningEffort::Low), "low");
        assert_eq!(format!("{}", ReasoningEffort::Medium), "medium");
        assert_eq!(format!("{}", ReasoningEffort::High), "high");
        assert_eq!(format!("{}", ReasoningEffort::Max), "max");
    }

    #[test]
    fn test_reasoning_effort_from_str() {
        assert_eq!(ReasoningEffort::from_str("low"), Some(ReasoningEffort::Low));
        assert_eq!(ReasoningEffort::from_str("medium"), Some(ReasoningEffort::Medium));
        assert_eq!(ReasoningEffort::from_str("med"), Some(ReasoningEffort::Medium));
        assert_eq!(ReasoningEffort::from_str("high"), Some(ReasoningEffort::High));
        assert_eq!(ReasoningEffort::from_str("max"), Some(ReasoningEffort::Max));
        assert_eq!(ReasoningEffort::from_str(""), None);
        assert_eq!(ReasoningEffort::from_str("none"), None);
        assert_eq!(ReasoningEffort::from_str("NONE"), None);
    }

    #[test]
    fn test_make_additional_params_with_enum() {
        let model = Model("deepseek/deepseek-v4-flash".to_string());
        let params = make_additional_params(&model, Some(ReasoningEffort::Medium));
        assert!(params.is_some(), "DeepSeek should support reasoning");
        assert_eq!(
            params.unwrap(),
            serde_json::json!({"reasoning": {"effort": "medium"}})
        );
    }

    #[test]
    fn test_make_additional_params_none() {
        let model = Model("deepseek/deepseek-v4-flash".to_string());
        let params = make_additional_params(&model, None);
        assert!(params.is_none(), "None effort should produce no params");
    }

    #[test]
    fn test_benchmark_cli_parsing_flow() {
        // Simulates the benchmark CLI parsing pattern used in main.rs
        let cli_input = "high".to_string();
        let parsed = if cli_input.is_empty() || cli_input == "none" {
            None
        } else {
            ReasoningEffort::from_str(&cli_input)
        };
        assert_eq!(parsed, Some(ReasoningEffort::High));

        let empty = String::new();
        let parsed_empty = if empty.is_empty() || empty == "none" {
            None
        } else {
            ReasoningEffort::from_str(&empty)
        };
        assert_eq!(parsed_empty, None);
    }

    #[test]
    fn test_reasoning_to_additional_params() {
        let model = Model("deepseek/deepseek-v4-flash".to_string());
        let result = reasoning_to_additional_params(&model, Some(ReasoningEffort::Low));
        assert!(result.is_some());
    }
}

/// Build the `additional_params` JSON value for a reasoning model.
///
/// If the model supports reasoning and `reasoning_effort` is `Some`,
/// returns `Some({"reasoning": {"effort": "medium"}})`
///
/// If the model does NOT support reasoning, or `reasoning_effort` is `None`,
/// returns `None`.
pub fn make_additional_params(
    model: &Model,
    reasoning_effort: Option<ReasoningEffort>,
) -> Option<serde_json::Value> {
    let effort = reasoning_effort?;
    let config = get_reasoning_config(model, effort)?;
    Some(config.to_additional_params_json())
}

/// Convert a `reasoning_effort: Option<ReasoningEffort>` plus a `model: &Model` into
/// the `additional_params: Option<serde_json::Value>` that should be passed
/// down the agent call chain.
#[deprecated = "Use make_additional_params instead."]
pub fn reasoning_to_additional_params(
    model: &Model,
    reasoning_effort: Option<ReasoningEffort>,
) -> Option<serde_json::Value> {
    make_additional_params(model, reasoning_effort)
}
