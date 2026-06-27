# Pricing Specification

**Type:** Behavioral Spec
**Change:** rig-tokudo
**Status:** Draft

## 1. Purpose

Define the pricing configuration for `rig-tokudo`'s cost tracking, replacing
the custom `CostTracker` struct with tokudo's built-in `PricingLayer`.

## 2. Pricing Config

### 2.1 Configuration

Pricing is configured via `rig_tokudo::PricingConfig`, populated from
environment variables:

```rust
use rig_tokudo::PricingConfig;

let pricing_config = PricingConfig::new()
    .with_model_pricing(
        "deepseek/deepseek-v4-flash",
        read_env_f64("COST_AGENT_INPUT_PER_1M", 0.14),   // $0.14 per 1M input tokens
        read_env_f64("COST_AGENT_OUTPUT_PER_1M", 0.28),  // $0.28 per 1M output tokens
    )
    .with_model_pricing(
        "judge-model",
        read_env_f64("COST_JUDGE_INPUT_PER_1M", 0.14),
        read_env_f64("COST_JUDGE_OUTPUT_PER_1M", 0.28),
    );
```

### 2.2 Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `COST_AGENT_INPUT_PER_1M` | `0.14` | Input token price for reviewer agents ($/1M tokens) |
| `COST_AGENT_OUTPUT_PER_1M` | `0.28` | Output token price for reviewer agents ($/1M tokens) |
| `COST_JUDGE_INPUT_PER_1M` | `0.14` | Input token price for judge ($/1M tokens) |
| `COST_JUDGE_OUTPUT_PER_1M` | `0.28` | Output token price for judge ($/1M tokens) |

These are the **same env vars** currently used by `cost.rs` — no rename needed.

### 2.3 Model-Specific Pricing

`PricingConfig` supports per-model rates. Different reviewer models or judge
models can have different pricing. The model name used in `PricingConfig`
must match the model name passed to `client.completion_model(&name)`.

```rust
PricingConfig::new()
    .with_model_pricing("gpt-4o", 2.50, 10.00)
    .with_model_pricing("claude-opus-4", 15.00, 75.00)
    .with_model_pricing("deepseek/deepseek-v4-flash", 0.14, 0.28);
```

If a model is not explicitly configured, tokudo falls back to a default
pricing rate (configurable via `PricingConfig::default_rates()`).

## 3. Token Counting

### 3.1 Real Token Counts (Improvement)

**Before (custom `cost.rs`):** Token counts were estimated as
`text.chars().count() / 4` — a rough heuristic that doesn't account for
tokenization differences between models or languages.

**After (tokudo):** Tokudo extracts real token counts from the API provider's
response (e.g., `usage.prompt_tokens`, `usage.completion_tokens` from OpenAI).
This gives accurate per-call token counts.

### 3.2 What Is Tracked

Per LLM call, tokudo tracks:

| Metric | Source | Description |
|--------|--------|-------------|
| `input_tokens` | API response | Number of tokens in the prompt (after optional compression) |
| `output_tokens` | API response | Number of tokens in the completion |
| `cost_usd` | Computed | `input_tokens * input_price + output_tokens * output_price` |
| `model` | Config | The model used for this call |
| `cache_hit` | Internal | Whether the response was served from cache (cost = $0) |

### 3.3 Cost Summary Output

The cost summary is accessible through tokudo's observability output. The
existing `CostSummary` struct from `crb-reporting` should be preserved or
adapted to receive tokudo's cost data.

## 4. Cache Hit Cost Accounting

**Contract:** Cache hits are reported as zero-cost calls. The tokens are
recorded (from the cached response's metadata) but the cost is $0.00.

| Scenario | Input Cost | Output Cost | Total Cost |
|----------|-----------|-------------|------------|
| Cache miss (API call) | `tokens_in * input_rate` | `tokens_out * output_rate` | Sum |
| Cache hit | $0.00 | $0.00 | $0.00 |

## 5. Migration from CostTracker

### 5.1 What We Remove

| Custom `CostTracker` Method | Tokudo Replacement |
|-----------------------------|-------------------|
| `CostTracker::new()` | `OptimizedModel::with_pricing(config)` |
| `record_agent(tokens_in, tokens_out, cache_hit)` | *Automatic* — tokudo reads from API response |
| `record_judge(tokens_in, tokens_out, cache_hit)` | *Automatic* |
| `total_cost_usd()` | Accessible via tokudo's observability |
| `agent_cache_hit_rate()` | Accessible via tokudo's observability |
| `judge_cache_hit_rate()` | Accessible via tokudo's observability |
| `total_tokens()` | Accessible via tokudo's observability |
| `to_summary()` → `CostSummary` | Map from tokudo's cost data |

### 5.2 CostSummary Backwards Compatibility

The `crb-reporting::CostSummary` struct used for output reporting should
continue to exist. Its fields must be populated from tokudo's cost data
(accessed through the observable pipeline or a direct query on the
`OptimizedModel`).

```rust
pub struct CostSummary {
    pub agent_tokens_in: usize,
    pub agent_tokens_out: usize,
    pub judge_tokens_in: usize,
    pub judge_tokens_out: usize,
    pub total_usd: f64,
    pub agent_cache_hit_rate: f64,
    pub judge_cache_hit_rate: f64,
}
```

## 6. Error Handling

| Scenario | Behavior |
|----------|----------|
| Unknown model name in pricing config | Falls back to default rate (configurable) |
| API doesn't return token usage | Falls back to char/4 estimation (warning logged) |
| Environment variable not set | Uses declared default |
| Env var set to invalid float | Uses declared default (warning logged) |
