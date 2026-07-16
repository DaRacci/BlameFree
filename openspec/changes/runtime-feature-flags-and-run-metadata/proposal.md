# Proposal: Runtime Feature Flags and Run Metadata

**Change ID:** runtime-feature-flags-and-run-metadata
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-07-16

## Summary

Replace compile-time `#[cfg(feature = "...")]` and `cfg!(feature = "...")` gates with runtime-configurable boolean flags, and add a structured `RunMetadata` section to benchmark and ad-hoc run outputs to capture the configuration context that produced each run.

## Motivation

### Compile-time flags are inflexible

The project uses three experimental feature flags (`exp14_template_vars`, `exp14_submit_finding`, `exp16_adaptive_agents`) gated at compile time via Cargo features, plus a `binary` flag for the binary-vs-lib split. This means:

- **Deployments are monolithic** — you must build a separate binary for each flag combination.
- **No runtime toggling** — operators cannot enable/disable features per-run without recompilation.
- **Expensive rebuilds** — testing a feature requires building from source.
- **Configuration is invisible** — there is no record of which flags were active when a run was executed.

Additionally, the `reduce_diff` frontend configuration is **already a runtime field** (`AppConfig.reduce_diff_enabled` on the frontend) — no compile-time gate exists for it in any Cargo.toml. It is excluded from this change.

### Run outputs lack configuration context

Benchmark and ad-hoc runs produce JSON results with PR-level metrics, costs, and token usage, but **nowhere do they record**:

- Which feature flags were enabled
- Which model was used (only stored implicitly in the directory name / summary)
- Which roles were selected
- Reasoning effort level
- Prompt version(s) used
- Tool preamble / budget configuration
- Git commit hash of the harness itself
- Any other configuration that affects result reproducibility

Without this metadata, comparing results across runs is ambiguous — was the difference due to model choice, feature flag changes, prompt updates, or actual algorithm improvements?

## Scope

### In scope

- **Feature flag conversion**: Replace `cfg!(feature = "exp14_*")` / `cfg!(feature = "exp16_*")` gates with runtime `bool` checks against a `RuntimeConfig` struct. Flags to convert: `exp14_template_vars`, `exp14_submit_finding`, `exp16_adaptive_agents`.
- **`RuntimeConfig` struct**: Holds boolean flags for each experimental feature, initialized at startup from CLI args / env vars.
- **`RunMetadata` struct**: Define and serialize a structured metadata block that records the configuration context of each run.
- **Benchmark run metadata**: Wire `RunMetadata` into the benchmark run output (JSON summary, per-PR result files).
- **Ad-hoc run metadata**: Wire `RunMetadata` into the ad-hoc review output.
- **Dashboard events**: Extend `RunFinished` (and add `RunStarted`) events to carry metadata.
- **WebUI display**: Show metadata in the WebUI run detail page.
- **JSON output**: Include metadata in all run-related JSON output files.
- **Serialization**: Metadata is serialized alongside existing JSON structures with `#[serde(default)]` for backward compatibility.

Note: `binary` is a compilation-only feature (`#[cfg(feature = "binary")]`) that gates the binary entrypoint and `ReviewArgs`. It has no runtime equivalent — it controls whether `crb-harness` exposes a `main()` or is only a library. It is excluded from runtime conversion.

### Out of scope (non-goals)

- Changing the **implementations** behind the feature gates (the actual logic remains unchanged).
- Changing pipeline logic, concurrency model, cache architecture, or agent orchestration.
- Converting the `binary` feature flag (it is purely a compilation concern).
- Adding new features or behaviors behind new flags.
- Database schema changes (metadata is file-based JSON only).
- Frontend redesign beyond adding metadata sections to existing pages.

## Key Design Decisions

1. **`RuntimeConfig` as a global singleton** — A `RuntimeConfig` struct (impl `Default`) holds all runtime flags. It is initialized once at startup from CLI args / env vars and made available via `once_cell::sync::Lazy` or a similar pattern. Feature-gated functions check `RuntimeConfig::global().exp14_template_vars` instead of `cfg!(feature = "exp14_template_vars")`.
2. **`RunMetadata` as a standalone struct** — Not embedded in existing structs, but stored alongside them. In JSON output, it lives at a top-level `metadata` key so old parsers (which don't know about metadata) silently ignore it.
3. **Dual-path while migrating** — Feature flags remain in `Cargo.toml` during the transition. The `cfg!()` blocks are replaced with `if runtime_config.flag { ... } else { ... }`. Once all flags are converted, the Cargo.toml feature entries can be removed.
4. **Backward compatibility** — All new metadata fields use `#[serde(default)]`. Existing runs without metadata render as empty/absent metadata (not errors).
