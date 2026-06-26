# Change: Port Python Scripts to Rust Crates

## Intent
Port aggregate_findings.py and severity_auditor.py to standalone Rust crates (crb-aggregator, crb-auditor) with both library APIs and CLI entrypoints.

## Scope
Two new workspace crates with dual lib.rs+main.rs. Pure functional ports — no I/O in library code. CLIs replicate original argparse interfaces via clap.

## Approach
- crb-aggregator: parse_report() with 3 strategies, semantic_dedup(), aggregate_batch(), format_candidate(). CLI: --reports-dir, --output, --replace, --pr-filter, --archive.
- crb-auditor: apply_severity_auditor() with 3 inflated pattern categories + 3 never-downgrade categories. CLI: --findings, --output, --report.
- Shared types (Finding, Severity, Candidate) live in crb-agents for cross-crate use.
