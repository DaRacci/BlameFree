//! CLI entrypoint for the aggregator — replicates original aggregate_findings.py interface.
//!
//! Reads `*-v4.5-report.md` files from a directory, aggregates findings, and writes
//! output JSON.

use clap::Parser;
use crb_aggregator::{aggregate_batch, MAX_CANDIDATES_PER_PR};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "crb-aggregator", about = "Aggregate code-review findings from Phase 4 reports")]
struct Args {
    /// Directory containing *-v4.5-report.md files
    #[arg(long, required = true)]
    reports_dir: PathBuf,

    /// Path to write candidates.json
    #[arg(long, required = true)]
    output: PathBuf,

    /// Replace existing candidates for these PRs (default: merge)
    #[arg(long, default_value_t = false)]
    replace: bool,

    /// Comma-separated PR numbers to include
    #[arg(long)]
    pr_filter: Option<String>,

    /// Archive raw reports before parsing
    #[arg(long, default_value_t = false)]
    archive: bool,
}

fn repo_map(owner_repo: &str) -> String {
    match owner_repo {
        "keycloak" => "keycloak/keycloak",
        "getsentry" => "getsentry/sentry",
        "grafana" => "grafana/grafana",
        "discourse" => "discourse/discourse",
        "calcom" => "calcom/cal.com",
        _ => return format!("{}/unknown", owner_repo),
    }
    .to_string()
}

fn main() {
    let args = Args::parse();

    if !args.reports_dir.is_dir() {
        eprintln!("ERROR: Reports directory not found: {:?}", args.reports_dir);
        std::process::exit(1);
    }

    // Collect all reports matching *-v4.5-report.md
    let mut pr_reports: HashMap<String, String> = HashMap::new();
    let mut entries: Vec<PathBuf> = Vec::new();

    if let Ok(dir) = std::fs::read_dir(&args.reports_dir) {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    if name.ends_with("-v4.5-report.md") {
                        entries.push(path);
                    }
                }
            }
        }
    }

    entries.sort();

    for report_file in &entries {
        let stem = report_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .replace("-v4.5-report", "");

        // Extract owner_repo and pr_num from filename like "keycloak_pr36880"
        let (owner_repo, pr_num) = if let Some(pos) = stem.find("_pr") {
            let (o, p) = stem.split_at(pos);
            let p = p.trim_start_matches("_pr");
            (o.to_string(), p.to_string())
        } else {
            eprintln!("WARNING: Could not parse PR key from filename: {:?}", report_file);
            continue;
        };

        let repo_key = repo_map(&owner_repo);
        let pr_key = format!("{}/{}", repo_key, pr_num);

        let report_text = match std::fs::read_to_string(report_file) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("WARNING: Could not read {:?}: {}", report_file, e);
                continue;
            }
        };
        pr_reports.insert(pr_key, report_text);
    }

    // Filter by PR numbers if specified
    if let Some(ref filter) = args.pr_filter {
        let wanted: HashSet<String> = filter.split(',').map(|s| s.trim().to_string()).collect();
        pr_reports.retain(|k, _| k.split('/').last().map_or(false, |n| wanted.contains(n)));
    }

    println!("Collected {} report(s) from {:?}", pr_reports.len(), args.reports_dir);

    // Aggregate
    let (candidates, stats) = aggregate_batch(pr_reports, MAX_CANDIDATES_PER_PR, args.archive);

    let output_path = &args.output;

    // Merge with existing candidates if --replace not set
    let candidates_json: Value = if output_path.exists() && !args.replace {
        let existing_text = match std::fs::read_to_string(output_path) {
            Ok(t) => t,
            Err(_) => {
                eprintln!("WARNING: Could not read existing output, overwriting");
                String::new()
            }
        };
        let mut existing: serde_json::Value =
            serde_json::from_str(&existing_text).unwrap_or(Value::Object(serde_json::Map::new()));

        if let Value::Object(ref mut existing_map) = existing {
            for (k, v) in candidates {
                existing_map.insert(k, serde_json::to_value(v).unwrap_or(Value::Null));
            }
        }
        existing
    } else {
        let mut map = serde_json::Map::new();
        for (k, v) in candidates {
            map.insert(k, serde_json::to_value(v).unwrap_or(Value::Null));
        }
        Value::Object(map)
    };

    // Write output
    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let output_json = serde_json::to_string_pretty(&candidates_json).unwrap_or_default();
    match std::fs::write(output_path, &output_json) {
        Ok(_) => println!("Wrote {} PR(s) to {:?}", 
            candidates_json.as_object().map(|m| m.len()).unwrap_or(0), 
            output_path),
        Err(e) => {
            eprintln!("ERROR: Failed to write output: {}", e);
            std::process::exit(1);
        }
    }

    println!("{}", serde_json::to_string_pretty(&stats).unwrap_or_default());
}
