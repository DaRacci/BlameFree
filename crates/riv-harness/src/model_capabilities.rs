//! Model capability detection for reasoning/thinking support.
//!
//! Queries OpenRouter model API, caches discovered model IDs plus reasoning-capable subset.
//! A periodic refresh may replace the cached state on success while keeping the previous state on transient failure.

use rig_core::providers::openai::responses_api::ReasoningEffort;
use riv_types::wrappers::{Model, WrappedData};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};

/// Configuration for model reasoning/thinking support.
#[derive(Debug, Clone, Serialize)]
pub enum ReasoningConfig {
    /// OpenAI-style reasoning
    ReasoningEffort {
        /// Reasoning effort level.
        effort: ReasoningEffort,
    },

    /// Anthropic-style thinking
    Thinking {
        /// Token budget for thinking.
        budget_tokens: u16,
    },
}

impl ReasoningConfig {
    /// Convert this reasoning config into JSON additional_params.
    pub fn to_additional_params_json(&self) -> serde_json::Value {
        match self {
            Self::ReasoningEffort { effort } => {
                serde_json::json!({
                    "reasoning": {
                        "effort": effort
                    }
                })
            }
            Self::Thinking { budget_tokens } => {
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

/// Response from OpenRouter `GET /api/v1/models`.
#[derive(serde::Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

/// Single model entry from OpenRouter model API.
#[derive(Clone, serde::Deserialize)]
struct OpenRouterModel {
    id: String,
    /// If `Some`, model supports reasoning.
    reasoning: Option<serde_json::Value>,
}

/// Cached model discovery state.
#[derive(Default)]
struct ModelCacheState {
    /// All discovered model IDs in display order.
    available: Option<Vec<String>>,
    /// IDs of models that support reasoning.
    reasoning: Option<HashSet<String>>,
    /// Error from the last failed discovery attempt, if the cache is empty.
    last_error: Option<String>,
}

/// Global model capability cache.
static CACHE: LazyLock<RwLock<ModelCacheState>> =
    LazyLock::new(|| RwLock::new(ModelCacheState::default()));

/// Fetch models from OpenRouter, returning the discovered list plus reasoning-capable subset.
async fn fetch_models() -> Result<(Vec<String>, HashSet<String>), String> {
    let client = reqwest::Client::new();
    // When an API key is available, query the authenticated `/models/user` endpoint so
    // guardrail/provider restrictions narrow the discovered set. Some key types (e.g. project
    // keys) are not authorized for this endpoint and return 401; degrade to the public `/models`
    // catalog in that case so the full, live list is still usable.
    let request = match std::env::var("OPENROUTER_API_KEY").ok() {
        Some(key) => client
            .get("https://openrouter.ai/api/v1/models/user")
            .bearer_auth(key),
        None => client.get("https://openrouter.ai/api/v1/models"),
    };

    match fetch_models_from(request).await {
        Ok(models) => Ok(categorize(models)),
        Err(error) if error.starts_with("API returned 401") => {
            tracing::warn!(
                "OpenRouter authenticated models endpoint rejected the key ({error}); \
                 degrading to public /models catalog"
            );
            let public = client.get("https://openrouter.ai/api/v1/models");
            let models = fetch_models_from(public).await?;
            Ok(categorize(models))
        }
        Err(error) => Err(error),
    }
}

async fn fetch_models_from(
    request: reqwest::RequestBuilder,
) -> Result<Vec<OpenRouterModel>, String> {
    let response = request
        .send()
        .await
        .map_err(|error| format!("HTTP request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("API returned {}", response.status()));
    }

    response
        .json::<OpenRouterModelsResponse>()
        .await
        .map(|body| body.data)
        .map_err(|error| format!("Failed to parse response: {error}"))
}

/// Split discovered models into ordered IDs and reasoning-capable subset.
fn categorize(models: Vec<OpenRouterModel>) -> (Vec<String>, HashSet<String>) {
    let reasoning_ids: HashSet<String> = models
        .iter()
        .filter(|model| model.reasoning.is_some())
        .map(|model| model.id.clone())
        .collect();

    let mut seen = std::collections::BTreeSet::new();
    for model in &models {
        let trimmed = model.id.trim();
        if !trimmed.is_empty() {
            seen.insert(trimmed.to_string());
        }
    }
    let ordered: Vec<String> = seen.into_iter().collect();

    (ordered, reasoning_ids)
}

/// Apply a successful discovery result to the cache.
fn apply_success(ordered: Vec<String>, reasoning: HashSet<String>) {
    let mut cache = CACHE.write().expect("model cache poisoned");
    cache.available = Some(ordered);
    cache.reasoning = Some(reasoning);
    cache.last_error = None;
}

/// Record a discovery failure, clearing the cache so callers surface the error.
fn record_error(error: String) {
    let mut cache = CACHE.write().expect("model cache poisoned");
    cache.available = None;
    cache.reasoning = None;
    cache.last_error = Some(error);
}

/// Warm caches by querying OpenRouter model API.
///
/// On failure the cache is cleared so [`available_models`] surfaces the error.
pub async fn warm_model_cache() {
    match fetch_models().await {
        Ok((ordered, reasoning)) => {
            apply_success(ordered, reasoning);
            log_discovery_info();
        }
        Err(error) => {
            record_error(error.clone());
            tracing::warn!("OpenRouter model API unavailable ({error}).");
        }
    }
}

/// Refresh the model cache, treating failures as transient.
///
/// On success the cache is replaced.
/// On failure the previous state is kept and only the error is logged.
pub async fn refresh_model_cache() -> Result<(), String> {
    match fetch_models().await {
        Ok((ordered, reasoning)) => {
            apply_success(ordered, reasoning);
            log_discovery_info();
            Ok(())
        }
        Err(error) => {
            tracing::warn!("OpenRouter model refresh failed ({error}).");
            Err(error)
        }
    }
}

fn log_discovery_info() {
    let cache = CACHE.read().expect("model cache poisoned");
    if let Some(ids) = &cache.available {
        tracing::info!(
            count = ids.len(),
            reasoning_count = cache.reasoning.as_ref().map_or(0, HashSet::len),
            "OpenRouter models API: discovered usable models"
        );
    }
}

/// Return cached usable models, or an error describing why discovery failed.
pub fn available_models() -> Result<Vec<Model>, String> {
    let cache = CACHE.read().expect("model cache poisoned");
    match &cache.available {
        Some(ids) if !ids.is_empty() => Ok(ids.iter().cloned().map(Model).collect()),
        _ => Err(cache
            .last_error
            .clone()
            .unwrap_or_else(|| "Model discovery has not run yet".to_string())),
    }
}

/// Check whether model supports reasoning.
///
/// Falls back to a lightweight heuristic only for capability detection if discovery data is absent.
pub fn supports_reasoning(model: &Model) -> bool {
    let cache = CACHE.read().expect("model cache poisoned");
    match &cache.reasoning {
        Some(ids) => {
            ids.contains(model.get())
                || ids
                    .iter()
                    .any(|id| model.get().ends_with(id) || id.ends_with(model.get()))
        }
        None => heuristic_is_reasoning_model(model),
    }
}

/// Heuristic for reasoning-capable model IDs.
fn heuristic_is_reasoning_model(model: &Model) -> bool {
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

/// Given model string and reasoning effort, return provider-specific reasoning config.
pub fn get_reasoning_config(model: &Model, effort: ReasoningEffort) -> Option<ReasoningConfig> {
    if !supports_reasoning(model) {
        return None;
    }

    if model.is_claude() {
        return Some(ReasoningConfig::Thinking {
            budget_tokens: effort as u16,
        });
    }

    Some(ReasoningConfig::ReasoningEffort { effort })
}

/// Build `additional_params` JSON for reasoning-capable models.
pub fn make_additional_params(
    model: &Model,
    reasoning_effort: Option<ReasoningEffort>,
) -> Option<serde_json::Value> {
    let effort = reasoning_effort?;
    let config = get_reasoning_config(model, effort)?;
    Some(config.to_additional_params_json())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_models_errors_without_discovery() {
        // The global cache may have been populated by other tests
        // just confirm the error path for an empty cache is exercised via the lock default.
        let cache = CACHE.read().unwrap();
        if cache.available.is_none() {
            assert!(available_models().is_err());
        }
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
}
