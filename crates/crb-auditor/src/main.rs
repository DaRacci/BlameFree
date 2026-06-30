//! CLI entrypoint for the severity auditor - replicates original severity_auditor.py interface.
//!
//! Reads findings from a JSON file, applies severity auditing, and optionally
//! writes an audit report.

use clap::Parser;
use crb_auditor::{apply_severity_auditor, format_severity_audit_report};
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "crb-auditor",
    about = "Apply severity auditing to code-review findings"
)]
struct Args {
    /// Path to input JSON file containing findings
    #[arg(long, required = true)]
    findings: PathBuf,

    /// Path to write audited findings JSON (default: stdout)
    #[arg(long)]
    output: Option<PathBuf>,

    /// Path to write human-readable audit report (optional)
    #[arg(long)]
    report: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    // Read findings from input file
    let input_text = match std::fs::read_to_string(&args.findings) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ERROR: Could not read findings file {:?}: {}", args.findings, e);
            std::process::exit(1);
        }
    };

    let raw_findings: Vec<Map<String, Value>> = match serde_json::from_str(&input_text) {
        Ok(f) => f,
        Err(_) => {
            // Try as a top-level object with a findings key
            match serde_json::from_str::<Map<String, Value>>(&input_text) {
                Ok(obj) => {
                    if let Some(arr) = obj.get("findings").and_then(|v| v.as_array()) {
                        let mut result = Vec::new();
                        for item in arr {
                            if let Some(map) = item.as_object() {
                                result.push(map.clone());
                            }
                        }
                        result
                    } else {
                        eprintln!("ERROR: Input JSON must be an array of findings or an object with a 'findings' key");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("ERROR: Failed to parse input JSON: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    eprintln!("Loaded {} findings from {:?}", raw_findings.len(), args.findings);

    // Apply severity auditor
    let findings_before: Vec<Map<String, Value>> = raw_findings.iter().cloned().collect();
    let findings_after = apply_severity_auditor(raw_findings);

    // Generate audit report if requested
    if let Some(ref report_path) = args.report {
        let report = format_severity_audit_report(&findings_before, &findings_after);
        if let Some(parent) = report_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(report_path, &report) {
            Ok(_) => eprintln!("Wrote audit report to {:?}", report_path),
            Err(e) => eprintln!("WARNING: Could not write report: {}", e),
        }
    }

    // Write audited findings
    let output_json = serde_json::to_string_pretty(&findings_after).unwrap_or_default();
    match args.output {
        Some(ref output_path) => {
            if let Some(parent) = output_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(output_path, &output_json) {
                Ok(_) => eprintln!("Wrote {} audited findings to {:?}", findings_after.len(), output_path),
                Err(e) => {
                    eprintln!("ERROR: Could not write output: {}", e);
                    std::process::exit(1);
                }
            }
        }
        None => {
            // Write to stdout
            println!("{}", output_json);
        }
    }
}
