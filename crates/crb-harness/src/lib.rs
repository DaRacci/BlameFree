//! Code Review Benchmark Harness library.
//!
//! Provides the public API for PR review (`review_pr`, `review_diff`) as well
//! as the internal orchestration functions used by the `benchmark` subcommand.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use crb_agents::build_agent;
use crb_agents::prompts::PromptLibrary;
use crb_auditor::apply_severity_auditor;
use crb_consensus::{CacheBackend, evaluate_pr_with_consensus};
use crb_dashboard::DashboardEvent;
use crb_judge::{compute_metrics, run_judge};
use crb_reporting::PrResult;
use crb_reporting::golden::GoldenCommentEntry;
use crb_rules::RuleSet;
use crb_shared::deduplicate::semantic_dedup;
use crb_shared::finding::Finding;
use crb_shared::jaccard::jaccard_similarity;
use crb_shared::sanitize_filename;
use crb_tools::create_linter_tool;
use crb_tools::linters::config::LinterConfig;
use crb_tools::linters::tool::LinterArgs;
use regex::Regex;
use rig_core::agent::{Agent, PromptResponse};
use rig_core::client::ProviderClient;
use rig_core::completion::{Prompt, Usage};
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use rig_core::tool::Tool;
use tokio::sync::broadcast;
use tracing::{info, info_span};

pub use crate::config::ReviewArgs;
pub mod cache;
pub mod config;
pub mod cost;
pub mod model_capabilities;
pub mod paths;
pub mod validation;

pub use cache::LlmCache;
pub use cache::RunHistoryEntry;
pub use cost::CostTracker;

/// Describes which kind of diff to review.
pub enum ReviewMode {
    /// Review a commit range `base..head`.
    Commits { base: String, head: String },

    /// Review the current working tree (unstaged + staged).
    Working,
}

/// Parameters for a full PR review.
pub struct ReviewParams {
    pub diff: String,
    pub model: String,
    pub pr_title: String,
    pub roles: Vec<String>,
    pub max_findings: usize,
    pub cache_dir: Option<PathBuf>,
}

/// File path patterns whose entire diff sections are always stripped.
const FILTERED_FILE_PATTERNS: &[&str] = &[
    // Lock files
    "pnpm-lock.yaml",
    "package-lock.json",
    "yarn.lock",
    "Cargo.lock",
    "Gemfile.lock",
    "composer.lock",
    "Pipfile.lock",
    "poetry.lock",
    "bun.lockb",
    "deno.lock",
    "flake.lock",
    // Vendor / dependency directories
    "/node_modules/",
    "/vendor/",
    "/Pods/",
    // Build output directories
    "/dist/",
    "/build/",
    "/.next/",
    "/.nuxt/",
    // Minified assets
    ".min.js",
    ".min.css",
    // Source maps
    ".map",
    // Coverage reports
    "/coverage/",
    "/htmlcov/",
    // Jest / test snapshots
    "__snapshots__/",
];

/// Check whether `path` (from a `diff --git a/path b/path` header) matches
/// any of the filtered patterns.
fn is_filtered_path(path: &str) -> bool {
    FILTERED_FILE_PATTERNS.iter().any(|pat| {
        // Direct match: path contains the pattern or ends with it
        if path.contains(pat) || path.ends_with(pat) {
            return true;
        }
        // For patterns starting with '/', also check without the leading slash
        // (git diff paths are relative, e.g. "node_modules/pkg/index.js")
        if let Some(stripped) = pat.strip_prefix('/') {
            if path.contains(stripped) || path.starts_with(stripped) || path.ends_with(stripped) {
                return true;
            }
        }
        false
    })
}

/// Count the categories of filtered files (for the summary note).
#[derive(Default)]
struct FilterCounts {
    lock: usize,
    vendor: usize,
    build: usize,
    minified: usize,
    map: usize,
    coverage: usize,
    snapshot: usize,
    other: usize,
}

impl FilterCounts {
    fn total(&self) -> usize {
        self.lock
            + self.vendor
            + self.build
            + self.minified
            + self.map
            + self.coverage
            + self.snapshot
            + self.other
    }

    fn classify(path: &str) -> &'static str {
        let patterns: &[(&[&str], &str)] = &[
            (
                &[
                    "pnpm-lock.yaml",
                    "package-lock.json",
                    "yarn.lock",
                    "Cargo.lock",
                    "Gemfile.lock",
                    "composer.lock",
                    "Pipfile.lock",
                    "poetry.lock",
                    "bun.lockb",
                    "deno.lock",
                    "flake.lock",
                ],
                "lock",
            ),
            (&["/node_modules/", "/vendor/", "/Pods/"], "vendor"),
            (&["/dist/", "/build/", "/.next/", "/.nuxt/"], "build"),
            (&[".min.js", ".min.css"], "minified"),
            (&[".map"], "map"),
            (&["/coverage/", "/htmlcov/"], "coverage"),
            (&["__snapshots__/"], "snapshot"),
        ];
        for (pats, label) in patterns {
            for p in *pats {
                if path.contains(p) || path.ends_with(p) {
                    return label;
                }
                // For patterns starting with '/', also check relative paths
                if let Some(stripped) = p.strip_prefix('/') {
                    if path.contains(stripped)
                        || path.starts_with(stripped)
                        || path.ends_with(stripped)
                    {
                        return label;
                    }
                }
            }
        }
        "other"
    }

    fn add(&mut self, path: &str) {
        match Self::classify(path) {
            "lock" => self.lock += 1,
            "vendor" => self.vendor += 1,
            "build" => self.build += 1,
            "minified" => self.minified += 1,
            "map" => self.map += 1,
            "coverage" => self.coverage += 1,
            "snapshot" => self.snapshot += 1,
            _ => self.other += 1,
        }
    }

    fn fmt_note(&self) -> String {
        if self.total() == 0 {
            return String::new();
        }
        let mut parts: Vec<String> = Vec::new();
        if self.lock > 0 {
            parts.push(format!("{} lock", self.lock));
        }
        if self.vendor > 0 {
            parts.push(format!("{} vendor", self.vendor));
        }
        if self.build > 0 {
            parts.push(format!("{} build", self.build));
        }
        if self.minified > 0 {
            parts.push(format!("{} minified", self.minified));
        }
        if self.map > 0 {
            parts.push(format!("{} map", self.map));
        }
        if self.coverage > 0 {
            parts.push(format!("{} coverage", self.coverage));
        }
        if self.snapshot > 0 {
            parts.push(format!("{} snapshot", self.snapshot));
        }
        let detail = parts.join(", ");
        format!(
            "[{} files filtered: {} - see raw diff for details]",
            self.total(),
            detail
        )
    }
}

/// Extract the file path from a `diff --git a/path b/path` header line.
fn parse_diff_git_path(line: &str) -> Option<&str> {
    // Format: "diff --git a/some/path b/some/path"
    let line = line.trim();
    let rest = line.strip_prefix("diff --git a/")?;
    // Find the " b/" separator
    let end = rest.find(" b/")?;
    Some(&rest[..end])
}

/// Split a raw unified diff into per-file sections, returning the header
/// separator and section body for each.
///
/// Each section begins with `diff --git a/...` and extends until the next
/// `diff --git` or end-of-string.
fn split_diff_sections(diff: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut current_header = String::new();
    let mut current_body = String::new();

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            // Save previous section
            if !current_header.is_empty() || !current_body.is_empty() {
                sections.push((
                    std::mem::take(&mut current_header),
                    std::mem::take(&mut current_body),
                ));
            }
            current_header = line.to_string();
        } else if !current_header.is_empty() {
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        }
    }

    // Last section
    if !current_header.is_empty() || !current_body.is_empty() {
        sections.push((current_header, current_body));
    }

    sections
}

/// Filter out files matching FILTERED_FILE_PATTERNS from a raw diff.
/// Returns the filtered diff with a summary note at the top.
fn filter_files(diff: &str) -> String {
    let sections = split_diff_sections(diff);
    let mut counts = FilterCounts::default();
    let mut kept_sections: Vec<String> = Vec::new();

    for (header, body) in &sections {
        let path = parse_diff_git_path(header).unwrap_or("");
        if path.is_empty() || !is_filtered_path(path) {
            // Keep this section
            let mut section = header.clone();
            if !body.is_empty() {
                section.push('\n');
                section.push_str(body);
            }
            kept_sections.push(section);
        } else {
            counts.add(path);
        }
    }

    let note = counts.fmt_note();
    let mut result = String::new();
    if !note.is_empty() {
        result.push_str(&note);
        result.push('\n');
    }
    // Add a blank line after the note if we have content
    if !note.is_empty() && !kept_sections.is_empty() {
        result.push('\n');
    }
    for section in &kept_sections {
        result.push_str(section);
        result.push('\n');
    }
    result
}

/// Strip diff metadata and reduce context to -U1 for unified diffs.
///
/// 1. Reduces context to -U1: keeps 1 context line before/after changed lines
/// 2. Strips `diff --git` headers
/// 3. Strips `index` lines
/// 4. Strips trailing hunk context text (after `@@` line-count portion)
/// 5. Keeps `--- a/path`, `+++ b/path`, `new file mode`, `deleted file mode`,
///    `@@` hunk headers (with stripped context text)
#[cfg(feature = "reduce-diff")]
pub fn strip_diff_metadata(diff: &str) -> String {
    let mut result = Vec::new();
    let mut current_hunk_lines: Vec<&str> = Vec::new();
    let mut in_hunk = false;

    // Helper: strip trailing text after the @@ line-count portion
    // Header format: @@ -a,b +c,d @@ optional text
    let strip_hunk_header_text = |header: &str| -> String {
        let parts: Vec<&str> = header.split("@@").collect();
        // split on "@@" gives: ["", " -a,b +c,d ", " optional text"]
        // We want: @@ + middle + @@
        if parts.len() >= 3 {
            format!("@@{}@@", parts[1])
        } else {
            header.to_string()
        }
    };

    // Helper: flush the current hunk with -U1 reduction
    let flush_hunk = |hunk_lines: &[&str], output: &mut Vec<String>| {
        if hunk_lines.is_empty() {
            return;
        }

        // Split: first line is the @@ header, rest are body lines
        let header = hunk_lines[0];
        let body = &hunk_lines[1..];

        // Find first and last changed lines (+ or -)
        let first_changed = body
            .iter()
            .position(|l| l.starts_with('+') || l.starts_with('-'));
        let last_changed = body
            .iter()
            .rposition(|l| l.starts_with('+') || l.starts_with('-'));

        if let (Some(first), Some(last)) = (first_changed, last_changed) {
            // Determine start: 1 context line before first changed, or 0 if not enough
            let start = if first > 0 { first - 1 } else { 0 };
            // Determine end: 1 context line after last changed
            let end = if last + 2 < body.len() {
                last + 2
            } else {
                body.len()
            };

            // Emit the @@ header (stripped of trailing context)
            let stripped_header = strip_hunk_header_text(header);
            output.push(stripped_header);

            // Emit the reduced body lines
            for line in &body[start..end] {
                output.push(line.to_string());
            }
        } else {
            // No changed lines - keep hunk as-is
            output.push(header.to_string());
            for line in body {
                output.push(line.to_string());
            }
        }
    };

    for line in diff.lines() {
        // Skip diff --git and index lines entirely
        if line.starts_with("diff --git") || line.starts_with("index ") {
            continue;
        }

        if line.starts_with("@@ ") && line.contains(" @@") {
            // Start of a new hunk - flush previous hunk if any
            if in_hunk && !current_hunk_lines.is_empty() {
                flush_hunk(&current_hunk_lines, &mut result);
                current_hunk_lines.clear();
            }
            in_hunk = true;
            current_hunk_lines.push(line);
        } else if in_hunk {
            // Inside a hunk: collect body lines
            current_hunk_lines.push(line);
        } else {
            // Outside a hunk: pass through (e.g. ---, +++, new file mode, deleted file mode)
            result.push(line.to_string());
        }
    }

    // Flush the last hunk
    if !current_hunk_lines.is_empty() {
        flush_hunk(&current_hunk_lines, &mut result);
    }

    result.join("\n")
}

/// Filter a raw diff to remove noise files. Returns the filtered diff
/// with a summary note at the top if any files were removed.
pub fn preprocess_diff(raw_diff: &str) -> String {
    #[cfg(feature = "reduce-diff")]
    {
        let filtered = filter_files(raw_diff);
        strip_diff_metadata(&filtered)
    }
    #[cfg(not(feature = "reduce-diff"))]
    {
        raw_diff.to_string()
    }
}

/// Run the shared agent loop for a set of roles, collecting findings.
async fn run_agent_roles(
    client: &rig_core::providers::openai::Client,
    model: &str,
    diff: &str,
    roles: &[&str],
    max_findings: usize,
    prompt_lib: &PromptLibrary,
) -> Vec<Finding> {
    let mut all_findings = Vec::new();

    for &role in roles {
        // Build agent with embedded prompt library
        let agent = build_agent(
            client, model, role, None, prompt_lib, None, None, None, None,
        );

        // Call agent with the diff - get real token usage via extended_details
        match agent.prompt(diff).extended_details().await {
            Ok(resp) => {
                let response = resp.output;
                let _usage = resp.usage;
                match parse_agent_findings(&response) {
                    Ok(mut findings) => {
                        if findings.len() > max_findings {
                            findings.truncate(max_findings);
                        }
                        all_findings.append(&mut findings);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse agent response for role {}: {}", role, e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Agent call failed for role {}: {}", role, e);
            }
        }
    }

    all_findings
}

/// Entry point for reviewing a PR given its diff as a string.
///
/// Builds agents for each role, runs them with the diff, and returns findings.
pub async fn review_pr(params: ReviewParams) -> Result<Vec<Finding>> {
    // Create OpenAI client from env
    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow!("Failed to create OpenAI client: {e}"))?;

    // Parse roles
    let prompt_lib =
        crb_agents::prompts::PromptLibrary::new().expect("Embedded prompts should be available");

    let roles: Vec<&str> = if params.roles.is_empty() {
        prompt_lib.roles()
    } else {
        params.roles.iter().map(|r| r.as_str()).collect()
    };

    let findings = run_agent_roles(
        &client,
        &params.model,
        &params.diff,
        &roles,
        params.max_findings,
        &prompt_lib,
    )
    .await;

    Ok(findings)
}

/// Like `review_pr` but accepts a [`PromptLibrary`] for custom prompts.
pub async fn review_pr_with_prompt_lib(
    params: ReviewParams,
    prompt_lib: &crb_agents::prompts::PromptLibrary,
) -> Result<Vec<Finding>> {
    // Create OpenAI client from env
    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow!("Failed to create OpenAI client: {e}"))?;

    // Parse roles
    let roles: Vec<&str> = if params.roles.is_empty() {
        prompt_lib.roles()
    } else {
        params.roles.iter().map(|r| r.as_str()).collect()
    };

    let mut all_findings = run_agent_roles(
        &client,
        &params.model,
        &params.diff,
        &roles,
        params.max_findings,
        prompt_lib,
    )
    .await;

    all_findings = post_process_findings(&all_findings);

    Ok(all_findings)
}

/// Review a diff by running `git diff` in the given `path`, then
/// call `review_pr()` with the diff to get agent findings.
///
/// - `ReviewMode::Commits { base, head }` -> `git diff base..head`
/// - `ReviewMode::Working`                -> `git diff` (unstaged + staged)
///
/// Returns a vector of agent findings parsed from the LLM response.
pub async fn review_diff(args: ReviewArgs) -> Result<Vec<Finding>> {
    let diff = match args.commits {
        Some(ref range) => {
            let output = Command::new("git")
                .arg("diff")
                .arg(range)
                .current_dir(&args.path)
                .output()
                .map_err(|e| anyhow!("Failed to run git diff: {e}"))?;
            String::from_utf8_lossy(&output.stdout).to_string()
        }
        None => {
            let output = Command::new("git")
                .arg("diff")
                .current_dir(&args.path)
                .output()
                .map_err(|e| anyhow!("Failed to run git diff: {e}"))?;
            String::from_utf8_lossy(&output.stdout).to_string()
        }
    };

    if diff.is_empty() {
        info!("No diff found - returning empty findings");
        return Ok(Vec::new());
    }

    info!(
        "Loaded diff ({} bytes) from {}",
        diff.len(),
        args.path.display()
    );

    // Preprocess: filter noise files and chunk oversized diffs
    let diff = crate::preprocess_diff(&diff);

    let prompt_lib =
        crb_agents::prompts::PromptLibrary::new().expect("Embedded prompts should be available");

    // Build ReviewParams and call review_pr
    let roles = vec![
        "SA".to_string(),
        "CL".to_string(),
        "ARCH".to_string(),
        "SEC".to_string(),
    ];
    let params = ReviewParams {
        diff: diff.clone(),
        model: args.model.clone(),
        pr_title: "review".to_string(),
        roles,
        max_findings: 20,
        cache_dir: None,
    };
    review_pr_with_prompt_lib(params, &prompt_lib).await
}

// =========================================================================
// Moved from main.rs - public helpers
// =========================================================================

/// Extract owner, repo name, and PR number from a GitHub PR URL.
///
/// Expects URLs of the form `https://github.com/{owner}/{repo}/pull/{num}`.
/// Returns `None` if the URL doesn't match the expected pattern.
pub fn extract_pr_info(url: &str) -> Option<(String, String, u32)> {
    let re = Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/pull/(\d+)$").ok()?;
    let caps = re.captures(url)?;
    let owner = caps.get(1)?.as_str().to_string();
    let repo = caps.get(2)?.as_str().to_string();
    let pr_num: u32 = caps.get(3)?.as_str().parse().ok()?;
    Some((owner, repo, pr_num))
}

/// Load the diff for a PR from pre-extracted cached diff files.
///
/// Cached diffs live at `{benchmark_dir}/diffs/{owner}_{repo}_{pr_num}.diff`.
pub fn load_cached_diff(
    benchmark_dir: &Path,
    owner: &str,
    repo: &str,
    pr_num: u32,
) -> Option<String> {
    let diffs_dir = benchmark_dir.join("diffs");
    let diff_path = diffs_dir.join(format!("{}_{}_{}.diff", owner, repo, pr_num));
    match std::fs::read_to_string(&diff_path) {
        Ok(content) => {
            info!(
                "Loaded cached diff ({} bytes) from {}",
                content.len(),
                diff_path.display()
            );
            Some(content)
        }
        Err(e) => {
            tracing::warn!(
                "Cached diff not found at {}: {}. Using empty diff.",
                diff_path.display(),
                e
            );
            None
        }
    }
}

/// Parse an agent's LLM response string into a `Vec<Finding>`.
///
/// Attempts three strategies in order:
/// 1. Direct JSON array deserialization via `serde_json::from_str`.
/// 2. JSON extraction from markdown fenced code blocks (```json ... ```).
/// 3. Find any JSON array in the response.
///
/// Before deserializing, field names are normalised (path->file,
/// description->message, text->message, category->rule_code, component->file)
/// and severity values are case-normalised ("high"->"High", "MEDIUM"->"Medium").
///
/// If all strategies fail, returns an empty `Vec` with a warning.
pub fn parse_agent_findings(response: &str) -> Result<Vec<Finding>, String> {
    // Log raw response first for debugging
    let preview_len = std::cmp::min(500, response.len());
    info!(
        "Agent raw response (first 500 chars): {}",
        &response[..preview_len]
    );

    // Helper: normalise field names and severity in a JSON value array.
    fn normalise_findings(raw: &str) -> Option<Vec<Finding>> {
        let mut values: Vec<serde_json::Value> = serde_json::from_str(raw).ok()?;
        for v in &mut values {
            if let Some(obj) = v.as_object_mut() {
                // Normalise field aliases
                if let Some(val) = obj.remove("path") {
                    if !obj.contains_key("file") {
                        obj.insert("file".to_string(), val);
                    }
                }
                if let Some(val) = obj.remove("description") {
                    if !obj.contains_key("message") {
                        obj.insert("message".to_string(), val);
                    }
                }
                if let Some(val) = obj.remove("text") {
                    if !obj.contains_key("message") {
                        obj.insert("message".to_string(), val);
                    }
                }
                if let Some(val) = obj.remove("category") {
                    if !obj.contains_key("rule_code") {
                        obj.insert("rule_code".to_string(), val);
                    }
                }
                if let Some(val) = obj.remove("component") {
                    if !obj.contains_key("file") && !obj.contains_key("path") {
                        obj.insert("file".to_string(), val);
                    }
                }

                // Normalise severity case: "high" -> "High", "MEDIUM" -> "Medium"
                if let Some(sev) = obj.get("severity").and_then(|s| s.as_str()) {
                    let normalised = match sev.to_lowercase().as_str() {
                        "high" => "High",
                        "medium" | "med" => "Medium",
                        "low" => "Low",
                        "critical" | "crit" => "Critical",
                        "info" | "informational" => "Info",
                        _ => sev, // keep as-is
                    };
                    obj.insert(
                        "severity".to_string(),
                        serde_json::Value::String(normalised.to_string()),
                    );
                }
            }
        }
        serde_json::from_value(serde_json::Value::Array(values)).ok()
    }

    // Strategy 1: Try direct JSON array parse with normalisation
    if let Some(findings) = normalise_findings(response) {
        info!(
            "Parsed {} finding(s) directly from agent JSON response",
            findings.len()
        );
        return Ok(findings);
    }

    // Strategy 2: Extract JSON from markdown code blocks
    let re = Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n\s*```").unwrap();
    if let Some(caps) = re.captures(response) {
        let inner = caps.get(1).unwrap().as_str().trim();
        if let Some(findings) = normalise_findings(inner) {
            info!(
                "Parsed {} finding(s) from markdown code block in agent response",
                findings.len()
            );
            return Ok(findings);
        }
    }

    // Strategy 3: Find any JSON array in the response
    let array_re = Regex::new(r"\[[\s\S]*\]").unwrap();
    if let Some(m) = array_re.find(response) {
        if let Some(findings) = normalise_findings(m.as_str()) {
            info!(
                "Parsed {} finding(s) from embedded JSON array",
                findings.len()
            );
            return Ok(findings);
        }
    }

    // All strategies failed - warn and return empty
    let truncated = if response.len() > 200 {
        format!("{}...", &response[..200])
    } else {
        response.to_string()
    };
    tracing::warn!(
        "Failed to parse agent response as Finding array. \
         Response (truncated): {}",
        truncated
    );
    Ok(Vec::new())
}

// =========================================================================
// Internal orchestration functions (used by the benchmark subcommand)
// =========================================================================

/// Call an async function with exponential backoff retry.
#[doc(hidden)]
pub async fn with_retry<F, Fut, T, E>(f: F, max_retries: usize, base_delay_ms: u64) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0usize;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(e);
                }
                let delay = Duration::from_millis(base_delay_ms * 2u64.pow(attempt as u32));
                tracing::warn!(
                    "Attempt {}/{} failed: {}. Retrying in {}ms",
                    attempt,
                    max_retries,
                    e,
                    delay.as_millis()
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Run the original single-agent evaluation with finding collection.
#[doc(hidden)]
#[allow(trivial_casts)]
pub async fn evaluate_pr_single_agent(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    cache: Option<Arc<crate::cache::LlmCache>>,
    cost_tracker: Arc<crate::cost::CostTracker>,
    dashboard_tx: Option<&broadcast::Sender<DashboardEvent>>,
    additional_params: Option<serde_json::Value>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    // Pre-compute content-addressed cache key components
    let diff_hash = crate::cache::LlmCache::sha256(diff);
    let rules_hash = crate::cache::LlmCache::sha256(rules_preamble.unwrap_or(""));
    let judge_prompt_hash = crate::cache::LlmCache::sha256(crb_judge::JUDGE_PROMPT);
    let judge_model = ""; // We don't have judge_model here; it's baked into the judge Agent

    let mut agent_set = tokio::task::JoinSet::new();
    let prompt_lib = prompt_lib.clone();
    for role in prompt_lib.roles() {
        let client = client.clone();
        let model = model.to_string();
        let role = role.to_string();
        let diff = diff.to_string();
        let diff_hash = diff_hash.clone();
        let rules_hash = rules_hash.clone();
        let preamble = rules_preamble.map(String::from);
        let p_lib = prompt_lib.clone();
        #[allow(clippy::unnecessary_cast)]
        let cache_arc: Option<Arc<dyn CacheBackend>> =
            cache.clone().map(|c| c as Arc<dyn CacheBackend>);
        let ct = cost_tracker.clone();
        let tx = dashboard_tx.map(|t| t.clone());
        let additional_params = additional_params.clone();

        agent_set.spawn(async move {
            let span = info_span!("agent", role = %role);
            let _guard = span.enter();

            // Compute agent cache key
            let prompt_hash = crate::cache::LlmCache::sha256(p_lib.get(&role).unwrap_or(""));
            let agent_cache_key = crate::cache::LlmCache::compute_agent_key(
                &prompt_hash,
                &diff_hash,
                &model,
                &role,
                &rules_hash,
            );

            // Check cache first
            if let Some(ref c) = cache_arc {
                if let Some((cached_response, cached_usage)) =
                    c.lookup_agent_by_key_with_usage(&agent_cache_key)
                {
                    info!(
                        "CACHE HIT for agent role={} (key={})",
                        role,
                        &agent_cache_key[..12]
                    );
                    // Record usage from cache if available, otherwise empty usage
                    let usage = cached_usage.unwrap_or_default();
                    ct.record_agent(&usage, true);
                    // Send chunk + finished for cached response
                    if let Some(ref tx) = tx {
                        let _ = tx.send(DashboardEvent::AgentChunk {
                            role: role.clone(),
                            chunk: cached_response.clone(),
                        });
                        let result = parse_agent_findings(&cached_response);
                        let findings_count = result.as_ref().map(|v| v.len()).unwrap_or(0);
                        let _ = tx.send(DashboardEvent::AgentFinished {
                            role,
                            findings: findings_count,
                            success: result.is_ok(),
                        });
                    }
                    let result = parse_agent_findings(&cached_response);
                    return result;
                }
            }
            info!(
                "CACHE MISS for agent role={} (key={})",
                role,
                &agent_cache_key[..12]
            );
            // Cache miss - make API call

            let tool_preamble = crb_tools::tool_prompt_section(
                &role,
                &crb_tools::budget::ToolCallBudget::default(),
                &[],
            );
            let agent = build_agent(
                &client,
                &model,
                &role,
                preamble.as_deref(),
                &p_lib,
                None, // template_vars
                Some(&tool_preamble),
                None, // workdir - not available in single-agent path
                additional_params.clone(),
            );
            let result: Result<Vec<Finding>, String> = with_retry(
                || async {
                    let resp: PromptResponse = agent
                        .prompt(&diff)
                        .extended_details()
                        .await
                        .map_err(|e| e.to_string())?;
                    let response = resp.output;
                    let usage = resp.usage;

                    ct.record_agent(&usage, false);

                    // Send chunk for live response
                    if let Some(ref tx) = tx {
                        let _ = tx.send(DashboardEvent::AgentChunk {
                            role: role.clone(),
                            chunk: response.clone(),
                        });
                    }

                    // Cache the prompt+response with content-addressed key, including usage
                    if let Some(ref c) = cache_arc {
                        c.save_agent_with_key_and_usage(
                            &agent_cache_key,
                            &role,
                            &diff,
                            &response,
                            &usage,
                        );
                    }

                    let findings = parse_agent_findings(&response);
                    // Send finished event
                    if let Some(ref tx) = tx {
                        let findings_count = findings.as_ref().map(|v| v.len()).unwrap_or(0);
                        let _ = tx.send(DashboardEvent::AgentFinished {
                            role: role.clone(),
                            findings: findings_count,
                            success: findings.is_ok(),
                        });
                    }
                    findings
                },
                3,    // max_retries
                1000, // base_delay_ms
            )
            .await;
            // If the whole retry chain failed, send failed event
            if result.is_err() {
                if let Some(ref tx) = tx {
                    let _ = tx.send(DashboardEvent::AgentFinished {
                        role: role.clone(),
                        findings: 0,
                        success: false,
                    });
                }
            }
            result
        });
    }

    let mut all_findings: Vec<Finding> = linter_findings;
    while let Some(res) = agent_set.join_next().await {
        match res {
            Ok(Ok(mut findings)) => all_findings.append(&mut findings),
            Ok(Err(e)) => tracing::warn!("Agent failed: {e}"),
            Err(e) => tracing::warn!("Agent join error: {e}"),
        }
    }

    // Judge evaluation: compare each finding against golden comments
    let mut verdicts = Vec::new();
    let jaccard_threshold = 0.12;
    for finding in &all_findings {
        for gc in &pr.comments {
            let score = jaccard_similarity(&finding.message, &gc.comment, false);
            if score >= jaccard_threshold {
                info!(
                    "Jaccard match: finding='{}' golden='{}' score={:.2}",
                    &finding.message[..std::cmp::min(60, finding.message.len())],
                    &gc.comment[..std::cmp::min(60, gc.comment.len())],
                    score
                );
                verdicts.push(crb_judge::JudgeVerdict {
                    reasoning: format!(
                        "Matched by {:.0}% word overlap (Jaccard heuristic)",
                        score * 100.0
                    ),
                    match_: true,
                    confidence: score,
                });
                continue;
            }

            // Step 2: File/line pre-filter (if available)
            if let Some(golden_file) = &gc.file {
                if let Some(finding_file) = &finding.file {
                    if golden_file != finding_file {
                        continue; // file mismatch - skip
                    }
                }
            }

            // Compute judge cache key
            let judge_key = crate::cache::LlmCache::compute_judge_key(
                &judge_prompt_hash,
                &finding.message,
                &gc.comment,
                judge_model,
            );

            // Check judge cache first
            if let Some(ref c) = cache {
                if let Some(cached_verdict) = c.lookup_judge_by_key(&judge_key) {
                    info!("CACHE HIT for judge (key={})", &judge_key[..12]);
                    cost_tracker.record_judge_empty(true);
                    verdicts.push(cached_verdict);
                    continue;
                }
            }

            // Cache miss - make API call
            info!("CACHE MISS for judge (key={})", &judge_key[..12]);
            match with_retry(
                || run_judge(judge, &gc.comment, &finding.message),
                3,    // max_retries
                1000, // base_delay_ms
            )
            .await
            {
                Ok((verdict, usage)) => {
                    cost_tracker.record_judge(&usage, false);

                    // Cache the judge call if cache is active
                    if let Some(ref c) = cache {
                        let verdict_json = serde_json::to_string(&verdict).unwrap_or_default();
                        let _ = c.save_judge_with_key(
                            &judge_key,
                            &gc.comment,
                            &finding.message,
                            &verdict_json,
                        );
                    }
                    verdicts.push(verdict);
                }
                Err(e) => tracing::warn!("Judge call failed after retries: {e}"),
            }
        }
    }

    Ok((all_findings, verdicts))
}

/// Run the multi-agent consensus evaluation, merging linter findings.
#[doc(hidden)]
#[allow(trivial_casts)]
pub async fn evaluate_pr_consensus(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    roles: &str,
    max_findings: usize,
    cache: Option<Arc<crate::cache::LlmCache>>,
    cost_tracker: Arc<crate::cost::CostTracker>,
    workdir: Option<&str>,
    reasoning_effort: Option<&str>,
    dashboard_tx: Option<&broadcast::Sender<DashboardEvent>>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    // Parse comma-separated roles
    let parsed_roles: Vec<&str> = roles
        .split(',')
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
        .collect();

    // ── Adaptive agent dispatch (EXP-016) ──────────────────────────────
    // NOTE: This experimental feature is intentionally disabled because it
    // overrides user-selected roles with a single GEN agent, which:
    //   (a) violates user role selection expectations, and
    //   (b) prevents ARCH/AR agents from appearing in the results.
    // Feature flag is kept to avoid breaking builds that enable it,
    // but the override is suppressed to respect user-selected roles.
    #[cfg(feature = "exp16_adaptive_agents")]
    let parsed_roles: Vec<&str> = {
        // Only apply adaptive dispatch when user has explicitly opted in
        // by selecting only a single role; otherwise respect user's choice.
        if parsed_roles.len() == 1 && crb_consensus::should_use_single_agent(diff, 3, 200) {
            info!("EXP-016 adaptive dispatch: small PR, using single GEN agent");
            vec!["GEN"]
        } else {
            parsed_roles
        }
    };

    if diff.is_empty() {
        info!("No diff - returning empty result");
        return Ok((Vec::new(), Vec::new()));
    }

    // ── Pre-compute content-addressed cache key components ──────────────
    let first_role = parsed_roles.first().copied().unwrap_or("SA");
    let prompt_hash = crate::cache::LlmCache::sha256(prompt_lib.get(first_role).unwrap_or(""));
    let rules_hash = crate::cache::LlmCache::sha256(rules_preamble.unwrap_or(""));
    let judge_prompt_hash = crate::cache::LlmCache::sha256(crb_judge::JUDGE_PROMPT);
    let diff_hash = crate::cache::LlmCache::sha256(diff);
    let judge_model = "";

    // Compute tool preamble only when workdir is provided
    let tool_preamble = workdir.map(|_| {
        let default_budget = crb_tools::budget::ToolCallBudget::default();
        crb_tools::tool_prompt_section(first_role, &default_budget, &[])
    });

    info!(
        "Consensus pipeline: {} agent role(s), max {} findings per role",
        parsed_roles.len(),
        max_findings,
    );

    // ── Convert reasoning_effort to additional_params ──────────────────
    let additional_params =
        model_capabilities::reasoning_to_additional_params(model, reasoning_effort);
    if additional_params.is_some() {
        info!(
            "Reasoning effort enabled: {:?}",
            reasoning_effort.unwrap_or("medium")
        );
    }

    // ── Build template variables from diff and PR context (EXP-014) ──
    #[cfg(feature = "exp14_template_vars")]
    let template_vars: Option<&'static HashMap<String, serde_json::Value>> = {
        let language = crb_tools::language_detector::detect_primary_language(diff);
        let repo_name = crb_tools::language_detector::extract_repo_name(&pr.url);
        let lang_ref: &'static str = Box::leak(language.into_boxed_str());
        let repo_ref: &'static str = Box::leak(repo_name.into_boxed_str());
        let map: HashMap<String, serde_json::Value> = HashMap::from([
            (
                "language".to_string(),
                serde_json::Value::String(lang_ref.to_string()),
            ),
            (
                "repo".to_string(),
                serde_json::Value::String(repo_ref.to_string()),
            ),
            ("role".to_string(), serde_json::Value::String(String::new())),
        ]);
        Some(&*Box::leak(Box::new(map)))
    };

    #[cfg(not(feature = "exp14_template_vars"))]
    let template_vars = None;

    let (result, agent_usage, judge_usage, agent_api_calls, judge_api_calls, judge_cache_hits) =
        evaluate_pr_with_consensus(
            pr,
            diff,
            client,
            model,
            judge,
            rules_preamble,
            prompt_lib,
            template_vars,
            &parsed_roles,
            max_findings,
            cache.clone().map(|c| c as Arc<dyn CacheBackend>),
            &diff_hash,
            &prompt_hash,
            &rules_hash,
            &judge_prompt_hash,
            judge_model,
            tool_preamble.as_deref(),
            workdir,
            additional_params,
            dashboard_tx.map(|t| t.clone()),
        )
        .await?;

    let role_count = parsed_roles.len();
    if role_count > 0 {
        let per_agent = Usage {
            input_tokens: agent_usage.input_tokens / role_count as u64,
            output_tokens: agent_usage.output_tokens / role_count as u64,
            total_tokens: agent_usage.total_tokens / role_count as u64,
            cached_input_tokens: agent_usage.cached_input_tokens / role_count as u64,
            cache_creation_input_tokens: agent_usage.cache_creation_input_tokens
                / role_count as u64,
            reasoning_tokens: agent_usage.reasoning_tokens / role_count as u64,
            tool_use_prompt_tokens: agent_usage.tool_use_prompt_tokens / role_count as u64,
        };
        // First agent_api_calls are cache misses, the rest are cache hits
        for i in 0..role_count {
            let cache_hit = i >= agent_api_calls;
            cost_tracker.record_agent(&per_agent, cache_hit);
        }
    }

    // Judge usage: only cache misses have real usage data
    let judge_total = judge_api_calls + judge_cache_hits;
    if judge_total > 0 {
        let per_judge = if judge_api_calls > 0 {
            Usage {
                input_tokens: judge_usage.input_tokens / judge_api_calls as u64,
                output_tokens: judge_usage.output_tokens / judge_api_calls as u64,
                total_tokens: judge_usage.total_tokens / judge_api_calls as u64,
                cached_input_tokens: judge_usage.cached_input_tokens / judge_api_calls as u64,
                cache_creation_input_tokens: judge_usage.cache_creation_input_tokens
                    / judge_api_calls as u64,
                reasoning_tokens: judge_usage.reasoning_tokens / judge_api_calls as u64,
                tool_use_prompt_tokens: judge_usage.tool_use_prompt_tokens / judge_api_calls as u64,
            }
        } else {
            Usage::new()
        };
        for _ in 0..judge_api_calls {
            cost_tracker.record_judge(&per_judge, false);
        }
        // Cache hits have zero usage (no stored data)
        for _ in 0..judge_cache_hits {
            cost_tracker.record_judge_empty(true);
        }
    }

    info!(
        "Consensus pipeline: {} agent findings, {} linter findings, {} goldens",
        result.findings_count,
        linter_findings.len(),
        result.golden_count
    );

    // The consensus crate's PrResult contains the actual findings count.
    // We still need to return `all_findings` for post-processing compat,
    // but note that all_findings is empty when linters are skipped -
    // the findings_count will be derived from verdicts in the caller.
    let all_findings: Vec<Finding> = Vec::new();
    Ok((all_findings, result.verdicts))
}

/// Post-process findings through aggregator dedup and auditor severity checks.
#[doc(hidden)]
pub fn post_process_findings(findings: &[Finding]) -> Vec<Finding> {
    if findings.is_empty() {
        return findings.to_vec();
    }

    let deduped = semantic_dedup(findings.to_vec());
    let audited = apply_severity_auditor(deduped);
    let capped = {
        let max = 20;
        if audited.len() > max {
            info!("capping {} findings to {} candidates", audited.len(), max);
            audited.into_iter().take(max).collect()
        } else {
            audited
        }
    };

    capped
}

/// Evaluate a single PR, optionally using consensus orchestration and linters.
#[doc(hidden)]
pub async fn evaluate_pr_with_postprocessing(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    benchmark_dir: &Path,
    linter_configs: Option<&HashMap<String, LinterConfig>>,
    skip_consensus: bool,
    linters_only: bool,
    ruleset: Option<&RuleSet>,
    prompt_lib: &PromptLibrary,
    roles: &str,
    max_findings: usize,
    cache_dir: Option<&PathBuf>,
    dashboard_tx: Option<&broadcast::Sender<DashboardEvent>>,
    reasoning_effort: Option<&str>,
) -> Result<PrResult> {
    let cache: Option<Arc<crate::cache::LlmCache>> = if let Some(cache_dir) = cache_dir {
        let pr_key = sanitize_filename(&pr.pr_title);
        let c = Arc::new(
            crate::cache::LlmCache::new(cache_dir, &pr_key)
                .expect("Failed to create LLM cache directory"),
        );
        info!(
            "LLM cache enabled for PR '{}' at {}",
            pr.pr_title,
            c.dir().display()
        );
        Some(c)
    } else {
        info!("LLM cache disabled for PR '{}'", pr.pr_title);
        None
    };

    let cost_tracker = Arc::new(crate::cost::CostTracker::new());

    // Strategy: try persistent worktree first (gives full file context),
    // then fall back to cached diff only.
    let (diff, pr_repo_dir): (String, Option<std::path::PathBuf>) = match extract_pr_info(&pr.url) {
        Some((owner, repo, pr_num)) => {
            // Check for persistent per-PR worktree
            let worktree_path = benchmark_dir
                .join("worktrees")
                .join(format!("{owner}_{repo}_{pr_num}"));
            if worktree_path.join(".git").exists() {
                info!(
                    "Using persistent worktree at {} for PR #{}",
                    worktree_path.display(),
                    pr_num
                );
                let d = load_cached_diff(benchmark_dir, &owner, &repo, pr_num).unwrap_or_default();
                (d, Some(worktree_path))
            } else {
                let d = load_cached_diff(benchmark_dir, &owner, &repo, pr_num).unwrap_or_default();
                (d, None)
            }
        }
        None => {
            tracing::warn!(
                "Could not extract PR info from URL '{}'. Using empty diff.",
                pr.url
            );
            (String::new(), None)
        }
    };
    if diff.is_empty() {
        tracing::warn!("Empty diff for PR: {} (url: {})", pr.pr_title, pr.url);
    } else {
        info!("Loaded diff ({} bytes) for PR: {}", diff.len(), pr.pr_title);
    }

    let diff = crate::preprocess_diff(&diff);

    let mut linter_findings: Vec<Finding> = Vec::new();
    if let Some(configs) = linter_configs {
        let host_repo_path = pr_repo_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| benchmark_dir.to_string_lossy().to_string());
        let mut linter_set = tokio::task::JoinSet::new();
        for (_name, lconfig) in configs {
            let tool = create_linter_tool(lconfig);
            let args = LinterArgs {
                repo_path: host_repo_path.clone(),
            };
            linter_set.spawn(async move {
                let result = tool.call(args).await;
                result
            });
        }

        while let Some(res) = linter_set.join_next().await {
            match res {
                Ok(Ok(findings)) => linter_findings.extend(findings),
                Ok(Err(e)) => tracing::warn!("Linter failed: {e}"),
                Err(e) => tracing::warn!("Linter join error: {e}"),
            }
        }

        info!(
            "Found {} linter finding(s) for PR: {}",
            linter_findings.len(),
            pr.pr_title
        );
    }

    if linters_only {
        return Ok(PrResult {
            pr_title: pr.pr_title.clone(),
            url: pr.url.clone(),
            findings_count: linter_findings.len(),
            golden_count: pr.comments.len(),
            metrics: crb_judge::Metrics::default(),
            verdicts: vec![],
            cost: Some(cost_tracker.to_summary()),
        });
    }

    // ── Compute rules preamble from changed files ────────────────────────
    let rules_preamble = ruleset.map(|rs| rs.format_preamble(&[]));

    // ── Agent evaluation ──────────────────────────────────────────────────
    let pr_key = sanitize_filename(&pr.pr_title);

    // Send AgentStarted for each role
    if let Some(tx) = dashboard_tx {
        for role in ["SA", "CL", "AR", "SEC"] {
            let _ = tx.send(DashboardEvent::AgentStarted {
                pr_key: pr_key.clone(),
                role: role.to_string(),
            });
        }
    }

    let (all_findings, verdicts) = if skip_consensus {
        evaluate_pr_single_agent(
            pr,
            client,
            model,
            judge,
            &diff,
            linter_findings,
            rules_preamble.as_deref(),
            prompt_lib,
            cache.clone(),
            cost_tracker.clone(),
            dashboard_tx,
            None, // additional_params not wired for single-agent path
        )
        .await?
    } else {
        evaluate_pr_consensus(
            pr,
            client,
            model,
            judge,
            &diff,
            linter_findings,
            rules_preamble.as_deref(),
            prompt_lib,
            roles,
            max_findings,
            cache.clone(),
            cost_tracker.clone(),
            None,
            reasoning_effort,
            dashboard_tx,
        )
        .await?
    };

    // ── Post-processing: aggregator dedup + auditor severity check ────────
    let processed_findings = post_process_findings(&all_findings);

    // ── Send AgentFinished for each role ────────────────────────────────
    if let Some(tx) = dashboard_tx {
        for (i, role) in ["SA", "CL", "AR", "SEC"].iter().enumerate() {
            let role_findings = if skip_consensus {
                let per_role = all_findings.len() / 4;
                if i == 0 {
                    all_findings.len() - per_role * 3
                } else {
                    per_role
                }
            } else {
                processed_findings.len() / 4
            };
            let _ = tx.send(DashboardEvent::AgentFinished {
                role: role.to_string(),
                findings: role_findings,
                success: true,
            });
        }
    }

    let metrics = compute_metrics(&verdicts, pr.comments.len());

    if let Some(tx) = dashboard_tx {
        let tokens = cost_tracker.total_tokens();
        let total_tokens = tokens.0 + tokens.1;
        let cost_usd = cost_tracker.total_cost_usd();
        let total_agent_calls = 4;
        let _ = tx.send(DashboardEvent::PrCompleted {
            pr_key,
            metrics: metrics.clone(),
            cost: cost_usd,
            total_tokens,
            agent_calls: total_agent_calls,
            findings_count: verdicts.len(),
        });
    }

    let metadata = serde_json::json!({
        "pr_title": pr.pr_title,
        "url": pr.url,
        "model": model,
        "skip_consensus": skip_consensus,
        "timestamp": format!("{:?}", std::time::SystemTime::now()),
        "findings_count": verdicts.len(),
        "golden_count": pr.comments.len(),
        "metrics": {
            "true_positives": metrics.true_positives,
            "false_positives": metrics.false_positives,
            "false_negatives": metrics.false_negatives,
            "precision": metrics.precision,
            "recall": metrics.recall,
            "f1": metrics.f1,
        },
    });
    if let Some(ref cache) = cache {
        if let Err(e) = cache.save_metadata(&metadata) {
            tracing::warn!("Failed to write cache metadata: {e}");
        }
    }

    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: verdicts.len(),
        golden_count: pr.comments.len(),
        metrics,
        verdicts,
        cost: Some(cost_tracker.to_summary()),
    })
}

/// Append a run history entry to the `_runs.json` file in the cache directory.
#[doc(hidden)]
fn append_run_history(cache_dir: &Path, entry: &RunHistoryEntry) -> Result<()> {
    let path = cache_dir.join(crate::paths::RUNS_FILE);
    let mut runs: Vec<RunHistoryEntry> = if path.exists() {
        let content = std::fs::read_to_string(&path).unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| Vec::new())
    } else {
        Vec::new()
    };
    runs.push(entry.clone());
    std::fs::write(&path, serde_json::to_string_pretty(&runs)?)?;
    info!("Appended run history to: {}", path.display());
    Ok(())
}

/// Write the `_summary.json` aggregate statistics file to the cache directory.
#[doc(hidden)]
pub fn write_summary(
    cache_dir: &PathBuf,
    model: &str,
    judge_model: &str,
    results: &[PrResult],
    duration: Duration,
) -> Result<()> {
    let total_llm_calls: usize = results.iter().map(|r| r.findings_count).sum();
    let total_judge_calls: usize = results.iter().map(|r| r.verdicts.len()).sum();

    let total_tokens: usize = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.agent_tokens_in + c.agent_tokens_out + c.judge_tokens_in + c.judge_tokens_out)
        .sum();
    let total_cost_usd: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.total_usd)
        .sum();
    let avg_agent_cache_hit_rate = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.agent_cache_hit_rate)
            .sum::<f64>()
            / results.len() as f64
    };
    let avg_judge_cache_hit_rate = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.judge_cache_hit_rate)
            .sum::<f64>()
            / results.len() as f64
    };

    let aggregate_metrics = if results.is_empty() {
        serde_json::json!({})
    } else {
        let avg_precision =
            results.iter().map(|r| r.metrics.precision).sum::<f64>() / results.len() as f64;
        let avg_recall =
            results.iter().map(|r| r.metrics.recall).sum::<f64>() / results.len() as f64;
        let avg_f1 = results.iter().map(|r| r.metrics.f1).sum::<f64>() / results.len() as f64;
        serde_json::json!({
            "avg_precision": avg_precision,
            "avg_recall": avg_recall,
            "avg_f1": avg_f1,
            "total_true_positives": results.iter().map(|r| r.metrics.true_positives).sum::<usize>(),
            "total_false_positives": results.iter().map(|r| r.metrics.false_positives).sum::<usize>(),
            "total_false_negatives": results.iter().map(|r| r.metrics.false_negatives).sum::<usize>(),
        })
    };

    let summary = serde_json::json!({
        "run_id": std::env::current_dir()
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default(),
        "model": model,
        "judge_model": judge_model,
        "total_prs": results.len(),
        "total_llm_calls": total_llm_calls,
        "total_judge_calls": total_judge_calls,
        "duration_secs": duration.as_secs_f64(),
        "aggregate_metrics": aggregate_metrics,
        "total_tokens": total_tokens,
        "total_cost_usd": total_cost_usd,
        "agent_cache_hit_rate": avg_agent_cache_hit_rate,
        "judge_cache_hit_rate": avg_judge_cache_hit_rate,
    });

    let summary_path = cache_dir.join(crate::paths::SUMMARY_FILE);
    std::fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;
    info!("Cache summary written to: {}", summary_path.display());

    // Append a run history entry to _runs.json
    let run_entry = RunHistoryEntry {
        run_id: summary["run_id"].as_str().unwrap_or("").to_string(),
        timestamp: format!("{:?}", std::time::SystemTime::now()),
        model: model.to_string(),
        judge_model: judge_model.to_string(),
        total_prs: results.len(),
        duration_secs: duration.as_secs_f64(),
        total_cost_usd,
        total_tokens,
        agent_cache_hit_rate: avg_agent_cache_hit_rate,
        judge_cache_hit_rate: avg_judge_cache_hit_rate,
    };
    append_run_history(cache_dir, &run_entry)?;

    Ok(())
}

/// Print a terminal summary of cost and cache hit rates for all PRs.
#[doc(hidden)]
pub fn print_terminal_summary(results: &[PrResult]) {
    let separator = "═══════════════════════════════════════════════";
    println!("\n{separator}");

    let mut grand_total_tokens = 0usize;
    let mut grand_total_cost = 0.0f64;

    for result in results {
        let pr_label = extract_pr_info(&result.url)
            .map(|(owner, repo, num)| format!("{owner}/{repo}/{num}"))
            .unwrap_or_else(|| result.pr_title.clone());

        let f1 = result.metrics.f1;
        let findings_count = result.findings_count;

        if let Some(ref cost) = result.cost {
            let pr_tokens = cost.agent_tokens_in
                + cost.agent_tokens_out
                + cost.judge_tokens_in
                + cost.judge_tokens_out;
            let pr_cost = cost.total_usd;

            grand_total_tokens += pr_tokens;
            grand_total_cost += pr_cost;

            println!(
                " {}: F1={:.3}, {} findings, {:.1}K tokens, ${:.4}",
                pr_label,
                f1,
                findings_count,
                pr_tokens as f64 / 1000.0,
                pr_cost,
            );
        } else {
            println!(
                " {}: F1={:.3}, {} findings, -- tokens, $--",
                pr_label, f1, findings_count,
            );
        }
    }

    let total_agent_rate: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.agent_cache_hit_rate)
        .sum();
    let total_judge_rate: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.judge_cache_hit_rate)
        .sum();
    let pr_count_with_cost = results.iter().filter(|r| r.cost.is_some()).count();

    let avg_agent_rate = if pr_count_with_cost > 0 {
        total_agent_rate / pr_count_with_cost as f64
    } else {
        0.0
    };
    let avg_judge_rate = if pr_count_with_cost > 0 {
        total_judge_rate / pr_count_with_cost as f64
    } else {
        0.0
    };

    println!("{separator}");
    println!(
        " TOTAL: {} PR(s), {:.1}K tokens, ${:.4}",
        results.len(),
        grand_total_tokens as f64 / 1000.0,
        grand_total_cost,
    );
    println!(" Agent cache hit rate: {:.1}%", avg_agent_rate * 100.0);
    println!(" Judge cache hit rate: {:.1}%", avg_judge_rate * 100.0);
    println!("{separator}");
}

/// Run the validation pipeline: load baseline, read results from output dir,
/// compute average metrics, compare against thresholds, and exit with
/// the appropriate code (0 = pass, 1 = fail).
#[doc(hidden)]
pub async fn run_validate(workspace_root: &Path, version: &str) -> Result<()> {
    info!("Running validation against baseline v{version}");

    let baseline = crate::validation::load_baseline(workspace_root, version)?;
    info!("Loaded baseline for version: {}", baseline.version);

    let output_dir = workspace_root.join("output");
    let results_dir = if output_dir.exists() {
        output_dir
    } else {
        anyhow::bail!(
            "Output directory not found: {}. Run the harness first.",
            output_dir.display()
        );
    };

    let mut loaded_results: Vec<crb_judge::Metrics> = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&results_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read result: {}", path.display()))?;
        match serde_json::from_str::<crb_judge::Metrics>(&content) {
            Ok(metrics) => loaded_results.push(metrics),
            Err(e) => {
                tracing::warn!(
                    "Skipping {}: could not parse as Metrics: {e}",
                    path.display()
                );
            }
        }
    }

    if loaded_results.is_empty() {
        anyhow::bail!(
            "No valid PR result files found in {}",
            results_dir.display()
        );
    }

    let total_prs = loaded_results.len();
    let (avg_precision, avg_recall, avg_f1) =
        crate::validation::compute_average_metrics(&loaded_results);
    let val_result = crate::validation::validate_against_baseline(
        &baseline,
        total_prs,
        avg_precision,
        avg_recall,
        avg_f1,
    );
    crate::validation::print_validation_summary(
        &baseline,
        &val_result,
        avg_precision,
        avg_recall,
        avg_f1,
    );

    if val_result.in_threshold {
        info!("Validation PASSED - all metrics within baseline thresholds");
        Ok(())
    } else {
        Err(anyhow!(
            "Validation FAILED - metrics exceed baseline thresholds"
        ))
    }
}
