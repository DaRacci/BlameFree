//! Model capability detection for reasoning/thinking support.
//!
//! Queries OpenRouter model API once, caches discovered model IDs plus reasoning-capable subset.
//! Falls back to heuristic/default model list when API unreachable.

use rig_core::providers::openai::responses_api::ReasoningEffort;
use riv_shared::{DEFAULT_MODEL, DEFAULT_MODEL_PRO};
use riv_types::wrappers::{Model, WrappedData};
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use std::sync::OnceLock;

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

/// Cache of reasoning-capable model IDs.
static REASONING_MODEL_IDS: OnceLock<Option<HashSet<String>>> = OnceLock::new();

/// Cache of all discovered model IDs.
static AVAILABLE_MODEL_IDS: OnceLock<Option<Vec<String>>> = OnceLock::new();

/// Whether cache fell back to heuristic matching.
static USING_FALLBACK: OnceLock<bool> = OnceLock::new();

fn normalize_model_ids<I>(ids: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = BTreeSet::new();
    for id in ids {
        let trimmed = id.trim();
        if !trimmed.is_empty() {
            seen.insert(trimmed.to_string());
        }
    }

    let mut ordered = vec![DEFAULT_MODEL.to_string(), DEFAULT_MODEL_PRO.to_string()];
    ordered.extend(
        seen.into_iter()
            .filter(|id| id != DEFAULT_MODEL && id != DEFAULT_MODEL_PRO),
    );
    ordered
}

fn fallback_models() -> Vec<Model> {
    normalize_model_ids([
        DEFAULT_MODEL.to_string(),
        DEFAULT_MODEL_PRO.to_string(),
        "openai/gpt-5-mini".to_string(),
    ])
    .into_iter()
    .map(Model)
    .collect()
}

fn set_model_cache(result: Result<Vec<OpenRouterModel>, String>, info_suffix: &str) {
    match result {
        Ok(models) => {
            let reasoning_ids: HashSet<String> = models
                .iter()
                .filter(|model| model.reasoning.is_some())
                .map(|model| model.id.clone())
                .collect();
            let ordered_ids = normalize_model_ids(models.into_iter().map(|model| model.id));

            tracing::info!(
                count = ordered_ids.len(),
                reasoning_count = reasoning_ids.len(),
                "OpenRouter models API: discovered usable models{}",
                info_suffix
            );

            let _ = AVAILABLE_MODEL_IDS.set(Some(ordered_ids));
            let _ = REASONING_MODEL_IDS.set(Some(reasoning_ids));
            let _ = USING_FALLBACK.set(false);
        }
        Err(error) => {
            tracing::warn!(
                "OpenRouter model API unreachable ({}); using fallback heuristic/model list",
                error
            );
            let _ = AVAILABLE_MODEL_IDS.set(None);
            let _ = REASONING_MODEL_IDS.set(None);
            let _ = USING_FALLBACK.set(true);
        }
    }
}

/// Warm caches by querying OpenRouter model API.
pub async fn warm_model_cache() {
    if AVAILABLE_MODEL_IDS.get().is_some() && REASONING_MODEL_IDS.get().is_some() {
        return;
    }

    set_model_cache(fetch_models_async().await, "");
}

async fn fetch_models_async() -> Result<Vec<OpenRouterModel>, String> {
    let url = "https://openrouter.ai/api/v1/models";
    let response = reqwest::get(url)
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

/// Warm caches synchronously via blocking HTTP call.
pub fn warm_model_cache_blocking() {
    if AVAILABLE_MODEL_IDS.get().is_some() && REASONING_MODEL_IDS.get().is_some() {
        return;
    }

    set_model_cache(fetch_models_blocking(), " (blocking)");
}

fn fetch_models_blocking() -> Result<Vec<OpenRouterModel>, String> {
    let url = "https://openrouter.ai/api/v1/models";
    let response =
        reqwest::blocking::get(url).map_err(|error| format!("HTTP request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("API returned {}", response.status()));
    }

    response
        .json::<OpenRouterModelsResponse>()
        .map(|body| body.data)
        .map_err(|error| format!("Failed to parse response: {error}"))
}

/// Return cached usable models, or fallback defaults if discovery unavailable.
pub fn available_models() -> Vec<Model> {
    match AVAILABLE_MODEL_IDS.get() {
        Some(Some(ids)) if !ids.is_empty() => ids.iter().cloned().map(Model).collect(),
        _ => fallback_models(),
    }
}

/// Check whether model supports reasoning.
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

/// Heuristic fallback for reasoning-capable model IDs.
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
    fn test_available_models_fallback_contains_default_first() {
        let models = fallback_models();
        assert_eq!(models[0].get(), DEFAULT_MODEL);
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
