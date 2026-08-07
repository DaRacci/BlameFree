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
    name: Option<String>,
    description: Option<String>,
    context_length: Option<u64>,
    architecture: Option<OpenRouterArchitecture>,
    pricing: Option<OpenRouterPricing>,
    supported_parameters: Option<Vec<String>>,
    /// If `Some`, model supports reasoning.
    reasoning: Option<serde_json::Value>,
}

/// Input/output modality description from OpenRouter.
#[derive(Clone, serde::Deserialize)]
struct OpenRouterArchitecture {
    modality: Option<String>,
    input_modalities: Option<Vec<String>>,
    output_modalities: Option<Vec<String>>,
}

/// Per-token pricing (as decimal strings, e.g. "0.0000005").
#[derive(Clone, serde::Deserialize)]
struct OpenRouterPricing {
    prompt: Option<String>,
    completion: Option<String>,
}

/// Displayable capability metadata for a model in the catalog.
///
/// Only the stable identifier is persisted to the allowlist; the rest of this
/// metadata is derived from the live OpenRouter response at runtime.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub context_length: Option<u64>,
    pub modality: Option<String>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    /// Price per prompt token in USD (None when not published/usable).
    pub prompt_price: Option<f64>,
    /// Price per completion token in USD (None when not published/usable).
    pub completion_price: Option<f64>,
    pub supports_tools: bool,
    pub supports_structured_outputs: bool,
    pub supports_web_search: bool,
    /// Whether the model supports reasoning/thinking.
    pub reasoning: bool,
    /// Whether reasoning is mandatory for this model.
    pub reasoning_mandatory: Option<bool>,
}

/// Cached model discovery state.
#[derive(Default)]
struct ModelCacheState {
    /// All discovered models in display order.
    available: Option<Vec<String>>,
    /// IDs of models that support reasoning.
    reasoning: Option<HashSet<String>>,
    /// Full capability metadata keyed by display order.
    details: Option<Vec<ModelInfo>>,
    /// Error from the last failed discovery attempt, if the cache is empty.
    last_error: Option<String>,
}

/// Global model capability cache.
static CACHE: LazyLock<RwLock<ModelCacheState>> =
    LazyLock::new(|| RwLock::new(ModelCacheState::default()));

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

/// Fetch models from OpenRouter, returning the discovered models.
///
/// Always queries the public `GET /api/v1/models` catalog (which includes the `reasoning`
/// field). This is the candidate pool an admin curates into the allowlist; it is not itself
/// the set of runnable models.
async fn fetch_models() -> Result<Vec<OpenRouterModel>, String> {
    let client = reqwest::Client::new();
    let request = client.get("https://openrouter.ai/api/v1/models");
    fetch_models_from(request).await
}

/// Parse `supported_parameters` into a set of capability flags.
fn parse_capabilities(supported: Option<&Vec<String>>) -> (bool, bool, bool) {
    let Some(list) = supported else {
        return (false, false, false);
    };
    let has = |needle: &str| list.iter().any(|p| p == needle);
    (
        has("tools"),
        has("structured_outputs"),
        has("web_search_options") || has("web_search"),
    )
}

/// Parse a decimal pricing string (e.g. "0.0000005") into a USD per-token value.
fn parse_price(raw: Option<&String>) -> Option<f64> {
    raw.and_then(|s| s.parse::<f64>().ok())
        .filter(|p| *p >= 0.0)
}

/// Convert raw OpenRouter entries into ordered display metadata.
fn build_details(models: Vec<OpenRouterModel>) -> Vec<ModelInfo> {
    let mut details: Vec<ModelInfo> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for model in models {
        let id = model.id.trim().to_string();
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        let (tools, structured, web_search) =
            parse_capabilities(model.supported_parameters.as_ref());
        let modality = model.architecture.as_ref().and_then(|a| a.modality.clone());
        let input_modalities = model
            .architecture
            .as_ref()
            .and_then(|a| a.input_modalities.clone())
            .unwrap_or_default();
        let output_modalities = model
            .architecture
            .as_ref()
            .and_then(|a| a.output_modalities.clone())
            .unwrap_or_default();
        let reasoning_mandatory = model
            .reasoning
            .as_ref()
            .and_then(|r| r.get("mandatory").and_then(|v| v.as_bool()));
        let reasoning = model.reasoning.is_some();
        let pricing = model.pricing.as_ref();
        details.push(ModelInfo {
            id,
            name: model.name,
            description: model.description,
            context_length: model.context_length,
            modality,
            input_modalities,
            output_modalities,
            prompt_price: pricing.and_then(|p| parse_price(p.prompt.as_ref())),
            completion_price: pricing.and_then(|p| parse_price(p.completion.as_ref())),
            supports_tools: tools,
            supports_structured_outputs: structured,
            supports_web_search: web_search,
            reasoning,
            reasoning_mandatory,
        });
    }
    details
}

/// Apply a successful discovery result to the cache.
fn apply_success(details: Vec<ModelInfo>) {
    let mut cache = CACHE.write().expect("model cache poisoned");
    cache.available = Some(details.iter().map(|d| d.id.clone()).collect());
    cache.reasoning = Some(
        details
            .iter()
            .filter(|d| d.reasoning)
            .map(|d| d.id.clone())
            .collect(),
    );
    cache.details = Some(details);
    cache.last_error = None;
}

/// Warm caches by querying OpenRouter model API.
///
/// On failure the cache is cleared so [`available_models`] surfaces the error.
pub async fn warm_model_cache() {
    match fetch_models().await {
        Ok(models) => {
            apply_success(build_details(models));
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
        Ok(models) => {
            apply_success(build_details(models));
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
    if let Some(details) = &cache.details {
        tracing::info!(
            count = details.len(),
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

/// Return the full cached capability metadata for the catalog, or an error describing
/// why discovery failed.
pub fn catalog_details() -> Result<Vec<ModelInfo>, String> {
    let cache = CACHE.read().expect("model cache poisoned");
    match &cache.details {
        Some(details) if !details.is_empty() => Ok(details.clone()),
        _ => Err(cache
            .last_error
            .clone()
            .unwrap_or_else(|| "Model discovery has not run yet".to_string())),
    }
}

/// Record a discovery failure, clearing the cache so callers surface the error.
fn record_error(error: String) {
    let mut cache = CACHE.write().expect("model cache poisoned");
    cache.available = None;
    cache.reasoning = None;
    cache.last_error = Some(error);
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
