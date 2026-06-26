# Change: Rust Ports of Existing Python Scripts — `aggregate_findings.py` + `severity_auditor.py`

## Intent
Port two production Python modules — `aggregate_findings.py` (537 lines) and `severity_auditor.py` (646 lines) — to native Rust as library modules that the Rust harness (`crb-harness`) can import directly as function calls. Eliminate the Python subprocess boundary so finding aggregation and severity auditing happen in-process, with zero serialization overhead and zero Python runtime dependency at test time.

## Scope
Build two Rust modules inside `crb-harness/src/`:

- **`aggregator.rs`** — Port of `aggregate_findings.py`: report parsing (table, bullet, JSON), severity normalisation, semantic dedup (group-key then Jaccard), candidate formatting, batch aggregation.
- **`severity_auditor.rs`** — Port of `severity_auditor.py`: rule-based severity inflation detection (architecture_nits, hypothetical_theoretical, style_nits), NEVER_DOWNGRADE protection (security, data_integrity, correctness_bugs), severity adjustment, audit reporting.

Both modules operate on the existing Rust `Finding` struct (serde) already defined for the harness.

Out of scope:
- Porting CLI entrypoints to Rust — CLIs stay in Python for now; the Rust modules are library-only.
- Porting `scaffold_pr.sh` or any shell scripts.
- Changing the existing Python scripts — they continue to work as standalone CLIs.

## Approach
Pure functional port. Each Python function maps to a Rust function with the same signature semantics:

| Python | Rust |
|--------|------|
| `classify_severity(str) -> str` | `fn classify_severity(&str) -> Severity` |
| `normalize(str) -> str` | `fn normalize(&str) -> String` |
| `extract_function(str) -> str\|None` | `fn extract_function(&str) -> Option<String>` |
| `jaccard_similarity(str, str) -> float` | `fn jaccard_similarity(&str, &str) -> f64` |
| `semantic_dedup(list[dict]) -> list[dict]` | `fn semantic_dedup(Vec<Finding>) -> Vec<Finding>` |
| `format_candidate(dict) -> dict` | `impl Finding { fn format(&self) -> Candidate }` |
| `parse_report(str) -> list[dict]` | `fn parse_report(&str) -> Result<Vec<Finding>>` |
| `aggregate_batch(dict) -> tuple[dict, dict]` | `fn aggregate_batch(Map<Url, Vec<Report>>) -> (Map<Url, Candidates>, Stats)` |
| `SEVERITY_ORDER` | `enum Severity { Critical, High, Medium, Low }` |
| `INFLATED_PATTERNS` (3 categories) | `struct InflatedCategory { name, patterns: Vec<Regex>, downgrade_quantum }` |
| `NEVER_DOWNGRADE_PATTERNS` (3 categories) | `struct ProtectionCategory { name, patterns: Vec<Regex> }` |
| `apply_severity_auditor(list[dict]) -> list[dict]` | `fn apply_severity_auditor(Vec<Finding>) -> Vec<Finding>` |
| `format_severity_audit_report(list[dict], list[dict]) -> str` | `fn format_severity_audit_report(&[Finding], &[Finding]) -> String` |

No I/O in the library modules — file I/O stays in `main.rs` / `scaffolding.rs`. The Python CLIs remain as standalone executables; the Rust modules are used by the harness runtime only.
