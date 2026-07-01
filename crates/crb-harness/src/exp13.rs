//! EXP-013: v6 Minimal Pipeline Baseline configuration.
//!
//! This module is only compiled when the `exp13_v6_pipeline` feature flag is
//! active (default ON). It provides the experiment-specific defaults and
//! config loading as the standard pipeline baseline.

use std::path::{Path, PathBuf};

use anyhow::Context;
use crb_agents::prompts::PromptLibrary;
use serde::Deserialize;

// ── Default constants ───────────────────────────────────────────────────────

/// Default agent roles for the v6 pipeline.
pub const DEFAULT_ROLES: &str = "SA,CL,AR,SEC";

/// Default max findings per agent for the v6 pipeline.
pub const DEFAULT_MAX_FINDINGS: usize = 20;

/// Default relative path to the EXP-013 prompts directory.
pub const DEFAULT_PROMPTS_DIR: &str = "experiments/EXP-013/prompts";

/// Default relative path to the EXP-013 experiment root.
pub const EXP13_ROOT: &str = "experiments/EXP-013";

// ── Config deserialisation ──────────────────────────────────────────────────

/// Harness section of the EXP-013 config.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct HarnessConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_judge_model")]
    pub judge_model: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_roles_str")]
    pub roles: String,
    #[serde(default = "default_max_findings")]
    pub max_findings: usize,
}

fn default_model() -> String {
    "deepseek/deepseek-v4-flash".to_string()
}
fn default_judge_model() -> String {
    "deepseek/deepseek-v4-flash".to_string()
}
fn default_concurrency() -> usize {
    4
}
fn default_roles_str() -> String {
    DEFAULT_ROLES.to_string()
}
fn default_max_findings() -> usize {
    DEFAULT_MAX_FINDINGS
}

/// Gates section of the EXP-013 config.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct GatesConfig {
    #[serde(default = "default_critical_high_min_agents")]
    pub critical_high_min_agents: usize,
    #[serde(default = "default_medium_low_min_agents")]
    pub medium_low_min_agents: usize,
    #[serde(default = "default_max_candidates")]
    pub max_candidates_per_pr: usize,
    #[serde(default = "default_severity_auditor_enabled")]
    pub severity_auditor_enabled: bool,
    pub severity_auditor_config: Option<String>,
}

fn default_critical_high_min_agents() -> usize {
    2
}
fn default_medium_low_min_agents() -> usize {
    2
}
fn default_max_candidates() -> usize {
    20
}
fn default_severity_auditor_enabled() -> bool {
    true
}

/// Top-level EXP-013 config.
#[derive(Debug, Clone, Deserialize)]
pub struct Exp13Config {
    #[serde(default)]
    pub experiment: ExperimentMeta,
    #[serde(default)]
    pub harness: HarnessConfig,
    #[serde(default)]
    pub gates: GatesConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExperimentMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub baseline: String,
    #[serde(default = "default_target_f1")]
    pub target_f1: f64,
}

fn default_target_f1() -> f64 {
    35.0
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            judge_model: default_judge_model(),
            concurrency: default_concurrency(),
            roles: default_roles_str(),
            max_findings: default_max_findings(),
        }
    }
}

impl Default for GatesConfig {
    fn default() -> Self {
        Self {
            critical_high_min_agents: default_critical_high_min_agents(),
            medium_low_min_agents: default_medium_low_min_agents(),
            max_candidates_per_pr: default_max_candidates(),
            severity_auditor_enabled: default_severity_auditor_enabled(),
            severity_auditor_config: None,
        }
    }
}

impl Default for ExperimentMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            baseline: String::new(),
            target_f1: default_target_f1(),
        }
    }
}

impl Default for Exp13Config {
    fn default() -> Self {
        Self {
            experiment: ExperimentMeta::default(),
            harness: HarnessConfig::default(),
            gates: GatesConfig::default(),
        }
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Load the EXP-013 experiment configuration from the project's experiment
/// directory.
pub fn load_exp13_config() -> anyhow::Result<Exp13Config> {
    let project_root = locate_project_root()?;
    let config_path = project_root.join(EXP13_ROOT).join("config.toml");
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read EXP-013 config at {}", config_path.display()))?;
    let config: Exp13Config = toml::from_str(&content)
        .with_context(|| format!("Failed to parse EXP-013 config at {}", config_path.display()))?;
    Ok(config)
}

/// Load the EXP-013 prompt library embedded at compile time.
///
/// Uses the embedded prompts directory — no runtime disk loading needed.
pub fn load_exp13_prompt_library() -> PromptLibrary {
    PromptLibrary::new().expect("Embedded prompts should be available")
}

/// Return the path to the EXP-013 prompts directory if it exists.
pub fn exp13_prompts_dir() -> Option<PathBuf> {
    let root = locate_project_root().ok()?;
    let dir = root.join(DEFAULT_PROMPTS_DIR);
    if dir.exists() { Some(dir) } else { None }
}

/// Determine whether the candidate cap should be applied.
pub fn candidate_cap_enabled() -> bool {
    true
}

/// Determine whether the severity auditor should be applied.
pub fn severity_auditor_enabled() -> bool {
    true
}

/// The maximum number of candidates (findings) to emit per PR.
pub fn max_candidates_per_pr() -> usize {
    // Try to load from config; fall back to default on failure
    load_exp13_config()
        .map(|cfg| cfg.gates.max_candidates_per_pr)
        .unwrap_or(20)
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Try to locate the project root by walking up from the CWD looking for
/// `experiments/EXP-013/` or a `Cargo.toml` workspace marker.
fn locate_project_root() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()
        .context("Failed to determine current working directory")?;

    // Walk up from CWD looking for experiments/EXP-013
    let mut current = Some(cwd.as_path());
    while let Some(dir) = current {
        // Check if we found the project root (experiments/EXP-013 exists)
        let exp_dir = dir.join("experiments");
        if exp_dir.exists() && exp_dir.join("EXP-013").exists() {
            return Ok(dir.to_path_buf());
        }
        // Also check for Cargo.toml workspace marker
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            // Read the first few bytes to check for [workspace]
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return Ok(dir.to_path_buf());
                }
            }
        }
        current = dir.parent();
    }

    // Fallback: return CWD
    Ok(cwd)
}
