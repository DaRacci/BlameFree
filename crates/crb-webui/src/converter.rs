//! Converter module — replicates crb-adapter logic to convert per-PR JSON findings
//! to candidates.json format and optionally run the Python step3 judge.
//!
//! Scans `output/{run_id}/` for per-PR JSON files, converts findings to the
//! `candidates.json` format expected by `step3_judge_comments.py`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ── Public types ─────────────────────────────────────────────────────────────

/// Statistics returned after a successful conversion.
#[derive(Debug, Clone, Serialize)]
pub struct ConvertStats {
    pub run_id: String,
    pub pr_count: usize,
    pub finding_count: usize,
    pub candidates_path: String,
}

/// Result from running the Python judge.
#[derive(Debug, Clone, Serialize)]
pub struct JudgeResult {
    pub success: bool,
    pub message: String,
    pub stdout: String,
    pub stderr: String,
}

// ── Internal types (candidates.json format) ──────────────────────────────────

/// Top-level candidates.json structure: PR URL → tool name → findings
type Candidates = BTreeMap<String, ToolCandidates>;

/// Per-tool candidates for a single PR
#[derive(Debug, Clone, Serialize)]
struct ToolCandidates {
    hermes: Vec<CandidateFinding>,
}

/// A single finding in candidates.json format
#[derive(Debug, Clone, Serialize)]
struct CandidateFinding {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
    source: String,
}

// ── Input format (per-PR JSON) ──────────────────────────────────────────────

/// A finding from the harness.
#[derive(Debug, Clone, Deserialize)]
struct HarnessFinding {
    message: String,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    line: Option<u32>,
}

/// Generic per-PR JSON structure.
#[derive(Debug, Clone, Deserialize)]
struct PerPrFile {
    #[serde(default)]
    pr_url: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    findings: Option<Vec<HarnessFinding>>,
}

impl PerPrFile {
    fn pr_url_or_default(&self) -> Option<String> {
        self.pr_url.clone().or_else(|| self.url.clone())
    }
}

// ── Public functions ─────────────────────────────────────────────────────────

/// Convert per-PR JSON findings to candidates.json format in the run directory.
///
/// Returns conversion statistics on success, or an error message on failure.
pub fn convert_run(run_dir: &Path) -> Result<ConvertStats, String> {
    let run_id = run_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    if !run_dir.exists() {
        return Err(format!("Run directory not found: {:?}", run_dir));
    }

    // Scan for JSON files (skip summary.csv, candidates.json, run.log, etc.)
    let json_files: Vec<PathBuf> = match std::fs::read_dir(run_dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map_or(false, |ext| ext == "json")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map_or(false, |n| !n.starts_with("candidates"))
            })
            .collect(),
        Err(e) => return Err(format!("Failed to read directory {:?}: {}", run_dir, e)),
    };

    let mut candidates = Candidates::new();
    let mut total_findings = 0usize;

    for file_path in &json_files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "WARNING: Could not read {:?}: {}",
                    file_path.file_name().unwrap_or_default(),
                    e
                );
                continue;
            }
        };

        let per_pr: PerPrFile = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "WARNING: Could not parse {:?}: {}",
                    file_path.file_name().unwrap_or_default(),
                    e
                );
                continue;
            }
        };

        let Some(pr_url) = per_pr.pr_url_or_default() else {
            eprintln!(
                "WARNING: No 'url' or 'pr_url' field in {:?} — skipping",
                file_path.file_name().unwrap_or_default()
            );
            continue;
        };

        let hermes_findings: Vec<CandidateFinding> = match &per_pr.findings {
            Some(findings) => findings
                .iter()
                .map(|f| CandidateFinding {
                    text: f.message.clone(),
                    path: f.file.clone(),
                    line: f.line,
                    source: "extracted".to_string(),
                })
                .collect(),
            None => Vec::new(),
        };

        total_findings += hermes_findings.len();
        candidates.insert(
            pr_url,
            ToolCandidates {
                hermes: hermes_findings,
            },
        );
    }

    // Write candidates.json
    let output_path = run_dir.join("candidates.json");
    let json = serde_json::to_string_pretty(&candidates)
        .map_err(|e| format!("Failed to serialize candidates: {}", e))?;

    std::fs::write(&output_path, &json)
        .map_err(|e| format!("Failed to write {:?}: {}", output_path, e))?;

    Ok(ConvertStats {
        run_id,
        pr_count: candidates.len(),
        finding_count: total_findings,
        candidates_path: output_path.to_string_lossy().to_string(),
    })
}

/// Run the Python step3_judge_comments.py judge on the converted candidates.
///
/// Expects candidates.json to already exist in `run_dir`.
/// Uses `benchmark_dir` (path containing `offline/`) if provided, otherwise
/// falls back to heuristic search relative to run_dir and cwd.
pub async fn run_judge(
    run_dir: &Path,
    benchmark_dir: Option<&Path>,
) -> JudgeResult {
    let candidates_path = run_dir.join("candidates.json");
    if !candidates_path.exists() {
        return JudgeResult {
            success: false,
            message: "candidates.json not found — run convert first".to_string(),
            stdout: String::new(),
            stderr: String::new(),
        };
    }

    // Try to locate the offline/ directory
    let offline_dir = match benchmark_dir {
        Some(dir) => dir.join("offline"),
        None => {
            let run_dir_parent = run_dir.parent().unwrap_or(run_dir);
            let output_parent = run_dir_parent.parent().unwrap_or(run_dir_parent);
            let heuristic_dir = output_parent.join("offline");
            if heuristic_dir.exists() {
                heuristic_dir
            } else {
                let cwd_offline = PathBuf::from("offline");
                if cwd_offline.exists() {
                    cwd_offline
                } else {
                    return JudgeResult {
                        success: false,
                        message: "offline/ directory not found — cannot run judge".to_string(),
                        stdout: String::new(),
                        stderr: String::new(),
                    };
                }
            }
        }
    };

    let run_name = run_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output");

    // Run the Python judge via tokio::process::Command
    let output = tokio::process::Command::new("uv")
        .args([
            "run",
            "python",
            "-m",
            "code_review_benchmark.step3_judge_comments",
            "--tool",
            "hermes",
        ])
        .current_dir(&offline_dir)
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let success = out.status.success();

            let message = if success {
                format!("Python judge completed successfully for run '{}'", run_name)
            } else {
                format!(
                    "Python judge exited with status: {}",
                    out.status
                )
            };

            JudgeResult {
                success,
                message,
                stdout,
                stderr,
            }
        }
        Err(e) => JudgeResult {
            success: false,
            message: format!("Failed to run Python judge: {}", e),
            stdout: String::new(),
            stderr: String::new(),
        },
    }
}
