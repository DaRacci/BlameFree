//! Model capability detection for reasoning/thinking support.
//!
//! Queries the OpenRouter models API to discover which models support
//! reasoning, with a fallback heuristic when the API is unreachable.
//! Results are cached once per process via a [`tokio::sync::OnceCell`].

use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;

// ── Reasoning types (unchanged from the static registry) ────────────────────

/// Configuration for model reasoning/thinking support.
///
/// Contains all the information needed to inject reasoning parameters
/// into the API request body.
#[derive(Debug, Clone, Serialize)]
pub enum ReasoningConfig {
    /// OpenAI-style reasoning: `{"reasoning": {"effort": "low|medium|high"}}`
    /// Used by OpenAI o-series and DeepSeek models.
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

/// All supported reasoning effort levels.
pub const REASONING_EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "max"];

/// The reasoning effort level for OpenAI/DeepSeek style reasoning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Low,
    #[serde(rename = "medium")]
    Medium,
    High,
    Max,
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
    ///
    /// For OpenAI/DeepSeek style: `{"reasoning": {"effort": "medium"}}`
    /// For Anthropic style: `{"thinking": {"type": "enabled", "budget_tokens": 2048}}`
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

// ── API discovery types ──────────────────────────────────────────────────────

/// Response from OpenRouter's `GET /api/v1/models` endpoint.
#[derive(serde::Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

/// A single model entry from the OpenRouter models API.
#[derive(serde::Deserialize)]
struct OpenRouterModel {
    id: String,
    /// If `Some`, this model supports reasoning. The value is an object
    /// containing `max_tokens` and `max_output_tokens` limits.
    reasoning: Option<serde_json::Value>,
}

// ── Cached reasoning model IDs ───────────────────────────────────────────────

/// Lazily-initialised cache of model IDs that support reasoning.
///
/// Initialised on first call to [`supports_reasoning`] via a blocking HTTP
/// request, or falls back to heuristics on failure.
static REASONING_MODEL_IDS: Lazy<Mutex<Option<HashSet<String>>>> =
    Lazy::new(|| Mutex::new(None));

/// A flag that flips to `true` once the initial fetch has been attempted (even
/// on failure) so we never retry the API.
static INITIALISED: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// Whether we fell back to heuristic matching (API was unreachable).
static USING_FALLBACK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// Initialise the reasoning-model cache from the OpenRouter models API.
///
/// This is called automatically the first time [`supports_reasoning`] is
/// invoked.  It can also be called explicitly at startup if you want to
/// pre-warm the cache in a non-blocking way (e.g. in a background task).
pub fn ensure_initialised() {
    let mut done = INITIALISED.lock().unwrap();
    if *done {
        return;
    }
    *done = true;

    match fetch_reasoning_model_ids_blocking() {
        Ok(ids) => {
            let mut cache = REASONING_MODEL_IDS.lock().unwrap();
            *cache = Some(ids);
            tracing::info!(
                reason_model_count = cache.as_ref().map(|s| s.len()).unwrap_or(0),
                "OpenRouter model cache: reasoning-capable models loaded"
            );
        }
        Err(e) => {
            tracing::warn!(
                "OpenRouter model API unreachable ({}); using fallback heuristic",
                e
            );
            *USING_FALLBACK.lock().unwrap() = true;
        }
    }
}

/// Blocking HTTP call to fetch the list of reasoning-capable model IDs.
fn fetch_reasoning_model_ids_blocking() -> Result<HashSet<String>, String> {
    let url = "https://openrouter.ai/api/v1/models";
    let response = reqwest::blocking::get(url)
        .map_err(|e| format!("HTTP request failed: {e}"))?;

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

    tracing::info!(
        count = ids.len(),
        "OpenRouter models API: reasoning-capable models discovered"
    );

    Ok(ids)
}

// ── Public query functions ───────────────────────────────────────────────────

/// Check whether a model supports reasoning, consulting the cached OpenRouter
/// model list, with fallback to heuristic matching on failure.
///
/// The cache is lazily populated on the first call.  Returns `true` if the
/// model is known to support reasoning.
pub fn supports_reasoning(model: &str) -> bool {
    ensure_initialised();

    let cache = REASONING_MODEL_IDS.lock().unwrap();
    if let Some(ref ids) = *cache {
        // API cache is available – do an exact match against model IDs
        // (The user may pass the model id with or without a provider prefix.)
        ids.contains(model)
            || ids.iter().any(|id| {
                // Also check if the user-specified model string ends with our id
                // (e.g. user passes "openai/o3-mini" and API has "o3-mini")
                model.ends_with(id) || id.ends_with(model)
            })
    } else {
        // API was unreachable – use heuristic fallback
        fallback_is_reasoning_model(model)
    }
}

/// Heuristic fallback: models whose names contain certain keywords are assumed
/// to support reasoning.
fn fallback_is_reasoning_model(model: &str) -> bool {
    let model_lower = model.to_lowercase();
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
pub fn get_reasoning_config(model: &str, effort: Option<&str>) -> Option<ReasoningConfig> {
    if !supports_reasoning(model) {
        return None;
    }

    let model_lower = model.to_lowercase();

    // Claude models use Anthropic-style thinking
    if model_lower.contains("claude") {
        return Some(ReasoningConfig::Thinking {
            budget_tokens: 2048,
        });
    }

    // All other reasoning models (DeepSeek, OpenAI o-series, Gemini) use
    // OpenAI-style reasoning effort
    let effort = effort
        .and_then(ReasoningEffort::from_str)
        .unwrap_or(ReasoningEffort::Medium);

    Some(ReasoningConfig::ReasoningEffort { effort })
}

/// Build the `additional_params` JSON value for a reasoning model.
///
/// If the model supports reasoning and `reasoning_effort` is `Some`, returns
/// `Some({"reasoning": {"effort": "medium"}})` (or whatever effort level was
/// specified).
///
/// If the model does NOT support reasoning, or `reasoning_effort` is `None`,
/// returns `None`.
pub fn make_additional_params(
    model: &str,
    reasoning_effort: Option<&str>,
) -> Option<serde_json::Value> {
    let effort = reasoning_effort?;
    let config = get_reasoning_config(model, Some(effort))?;
    Some(config.to_additional_params_json())
}

// ── Convenience builder for the harness ──────────────────────────────────────

/// Convert a `reasoning_effort: Option<String>` plus a `model: &str` into
/// the `additional_params: Option<serde_json::Value>` that should be passed
/// down the agent call chain.
///
/// This is the function used by [`crate::evaluate_pr_consensus`] and friends.
pub fn reasoning_to_additional_params(
    model: &str,
    reasoning_effort: Option<&str>,
) -> Option<serde_json::Value> {
    match reasoning_effort {
        None => None,
        Some(effort) if effort.is_empty() => None,
        Some(effort) => make_additional_params(model, Some(effort)),
    }
}
