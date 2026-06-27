//! crb-adapter — Converts harness findings to candidates.json for Python step3 judge.
//!
//! Scans `output/{run_id}/` for per-PR JSON files, converts findings to the
//! `candidates.json` format expected by `step3_judge_comments.py`, and optionally
//! runs the judge directly.

use clap::Parser;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

// ── CLI ─────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "crb-adapter", about = "Convert harness findings to candidates.json for step3 judge")]
struct Args {
    /// Run ID — subdirectory under output-dir (e.g. "ca-test-1")
    #[arg(long, required = true)]
    run_id: String,

    /// Base output directory (default: "output")
    #[arg(long, default_value = "output")]
    output_dir: PathBuf,

    /// Also run the Python step3 judge after conversion
    #[arg(long, default_value_t = false)]
    judge: bool,
}

// ── Output format (candidates.json) ─────────────────────────────────────────

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

/// A finding from the harness. Flexible deserialization that accepts both the
/// existing harness format (used in crb-agents) and any extra fields.
/// Extra fields like `rule_code`, `severity`, and `source` are accepted but
/// not read — the adapter maps only message/file/line to the candidates format.
#[derive(Debug, Clone, serde::Deserialize)]
struct HarnessFinding {
    message: String,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    line: Option<u32>,
}

/// Generic per-PR JSON structure. Supports both:
/// - New format: `{ pr_url, findings: [...], ... }`
/// - Existing format: `{ url, ... }` (findings may be absent)
#[derive(Debug, Clone, serde::Deserialize)]
struct PerPrFile {
    /// New format field name
    #[serde(default)]
    pr_url: Option<String>,
    /// Existing format field name (used by PrResult)
    #[serde(default)]
    url: Option<String>,
    /// Findings array (may be absent in existing reports)
    #[serde(default)]
    findings: Option<Vec<HarnessFinding>>,
}

impl PerPrFile {
    fn pr_url_or_default(&self) -> Option<String> {
        self.pr_url.clone().or_else(|| self.url.clone())
    }
}

// ── Main ────────────────────────────────────────────────────────────────────

fn main() {
    let args = Args::parse();

    let run_dir = args.output_dir.join(&args.run_id);

    if !run_dir.exists() {
        eprintln!("ERROR: Run directory not found: {:?}", run_dir);
        std::process::exit(1);
    }

    // Scan for JSON files (skip summary.csv, candidates.json, run.log, etc.)
    let json_files: Vec<PathBuf> = match std::fs::read_dir(&run_dir) {
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
        Err(e) => {
            eprintln!("ERROR: Failed to read directory {:?}: {}", run_dir, e);
            std::process::exit(1);
        }
    };

    if json_files.is_empty() {
        eprintln!("WARNING: No JSON files found in {:?}", run_dir);
    }

    let mut candidates = Candidates::new();

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

        candidates.insert(pr_url, ToolCandidates {
            hermes: hermes_findings,
        });
    }

    // Write candidates.json
    let output_path = run_dir.join("candidates.json");
    let json = serde_json::to_string_pretty(&candidates)
        .expect("Failed to serialize candidates");

    std::fs::write(&output_path, &json)
        .unwrap_or_else(|e| {
            eprintln!("ERROR: Failed to write {:?}: {}", output_path, e);
            std::process::exit(1);
        });

    println!(
        "Wrote candidates.json with {} PR(s) to {:?}",
        candidates.len(),
        output_path
    );

    // Optionally run the Python judge
    if args.judge {
        run_judge(&run_dir);
    }
}

/// Run the Python step3_judge_comments.py from the offline directory.
fn run_judge(run_dir: &PathBuf) {
    // Try to determine the project root (parent of output/)
    let run_dir_parent = run_dir.parent().unwrap_or(run_dir);
    let output_parent = run_dir_parent.parent().unwrap_or(run_dir_parent);

    // The judge lives in the original Python offline directory.
    // Try common locations relative to the workspace root.
    let offline_dir = output_parent.join("offline");

    if !offline_dir.exists() {
        // Try relative to project root (output is at top level)
        let cwd_offline = PathBuf::from("offline");
        if cwd_offline.exists() {
            run_judge_in_dir(&cwd_offline, run_dir);
            return;
        }

        eprintln!(
            "WARNING: offline/ directory not found at {:?} or {:?} — skipping judge",
            offline_dir,
            PathBuf::from("offline").canonicalize().ok()
        );
        return;
    }

    run_judge_in_dir(&offline_dir, run_dir);
}

fn run_judge_in_dir(offline_dir: &PathBuf, run_dir: &PathBuf) {
    // Change to offline directory and run the Python judge
    let run_dir_abs = std::fs::canonicalize(run_dir).unwrap_or_else(|_| run_dir.clone());

    // Determine run directory basename for step3
    let run_name = run_dir_abs
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output");

    println!(
        "Running Python judge in {:?} (run: {})...",
        offline_dir, run_name
    );

    // Copy candidates.json to offline if needed
    let offline_candidates = offline_dir.join("candidates.json");
    if !offline_candidates.exists() {
        // Just reference it — judge expects it at its own working directory
        // We'll use --candidates-path or workdir
        eprintln!("NOTE: candidates.json should be accessible from the judge's working directory");
    }

    // Run the judge
    let status = std::process::Command::new("uv")
        .args([
            "run",
            "python",
            "-m",
            "code_review_benchmark.step3_judge_comments",
            "--tool",
            "hermes",
        ])
        .current_dir(offline_dir)
        .status();

    match status {
        Ok(s) if s.success() => println!("Python judge completed successfully."),
        Ok(s) => eprintln!("Python judge exited with status: {}", s),
        Err(e) => eprintln!("ERROR: Failed to run Python judge: {}", e),
    }
}
