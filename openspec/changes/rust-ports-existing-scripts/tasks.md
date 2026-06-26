# Tasks: Rust Ports of `aggregate_findings.py` + `severity_auditor.py`

## Module: `aggregator.rs`

### Infrastructure
- [ ] Create `crb-harness/src/aggregator.rs` with module structure
- [ ] Add `regex` and `once_cell` to `Cargo.toml` if not already present
- [ ] Add `mod aggregator;` to `lib.rs` or module tree
- [ ] Define `Stats` struct for batch aggregation results
- [ ] Define `MAX_CANDIDATES_PER_PR = 20` and `CROSS_VALIDATED_BADGE_THRESHOLD = 2` constants

### Core Functions
- [ ] `fn classify_severity(text: &str) -> Severity` — exact match, prefix match, default to Medium
- [ ] `fn normalize(text: &str) -> String` — lowercase, strip markdown, collapse whitespace
- [ ] `fn extract_function(text: &str) -> Option<String>` — 5 regex patterns in order; return `"file.func"` format where applicable
- [ ] `fn jaccard_similarity(a: &str, b: &str) -> f64` — HashSet-based word intersection/union
- [ ] `fn semantic_dedup(findings: Vec<Finding>) -> Vec<Finding>` — 3-tier dedup (group by (file,func), group by (file,line_bucket), Jaccard at 0.4 threshold)
- [ ] `fn format_candidate(finding: &Finding) -> Candidate` — severity badge, cross-validation label, source = "orchestrator_phase4"
- [ ] `fn parse_report(report_text: &str) -> Result<Vec<Finding>>` — dispatch to 3 strategies
- [ ] `fn _parse_table_format(report_text: &str) -> Vec<Finding>` — heading + table-row regex parsing
- [ ] `fn _parse_bullet_format(report_text: &str) -> Vec<Finding>` — severity sections, bullet items, prose
- [ ] `fn _parse_json_format(report_text: &str) -> Vec<Finding>` — direct JSON, embedded JSON, list/object variants
- [ ] `fn aggregate_batch(pr_reports: Map<Url, Report>) -> (Map<Url, Candidates>, Stats)` — full pipeline
- [ ] Error handling: return descriptive errors for malformed reports, partial parse results

### Regex Compilation
- [ ] Define `once_cell::sync::Lazy<Regex>` for: heading pattern, table-row field pattern, agent-code pattern
- [ ] Define `once_cell::sync::Lazy<Vec<Regex>>` for: extract-function patterns
- [ ] Define `once_cell::sync::Lazy<Regex>` for each bullet/prose variant pattern

## Module: `severity_auditor.rs`

### Infrastructure
- [ ] Create `crb-harness/src/severity_auditor.rs` with module structure
- [ ] Add `mod severity_auditor;` to module tree
- [ ] Define `InflatedCategory` struct with name, patterns, description, downgrade_quantum fields
- [ ] Define `ProtectionCategory` struct with name, patterns fields
- [ ] Define `INFLATED_CATEGORIES: Lazy<Vec<InflatedCategory>>` — 3 categories with all regex patterns

### Inflated Pattern Categories
- [ ] **architecture_nits** (~15 patterns): SRP, DIP, OCP, God class, feature envy, coupling, design pattern violations, refactoring suggestions, abstraction leaks. Quantum: -2
- [ ] **hypothetical_theoretical** (~12 patterns): could cause, might lead to, may result in, potential issue, future, in theory, theoretically, if not careful, what if, suppose, in some cases, might be/have/cause/lead. Quantum: -1
- [ ] **style_nits** (~8 patterns): naming convention, formatting, whitespace, indentation, cosmetic, could be simplified, could be cleaned up, minor nit/style/issue, magic number/string/value, hardcoded value. Quantum: -3

### Never-Downgrade Pattern Categories
- [ ] Define `NEVER_DOWNGRADE_CATEGORIES: Lazy<Vec<ProtectionCategory>>` — 3 categories
- [ ] **security_vulns** (~18 patterns): SQL injection, XSS, CSRF, auth bypass, privilege escalation, RCE, command injection, path traversal, SSRF, XXE, deserialization, IDOR, sensitive data exposure
- [ ] **data_integrity** (~6 patterns): data loss, data corruption, deadlock, livelock, race condition, transaction inconsistency, database corruption
- [ ] **correctness_bugs** (~10 patterns): wrong value/result, incorrect logic/condition, crash, null pointer, NPE, segfault, memory corruption/leak, index out of bounds, type error, key error

### Core Functions
- [ ] `fn severity_value(severity: &Severity) -> u8` — Critical=0, High=1, Medium=2, Low=3
- [ ] `fn has_never_downgrade_pattern(finding: &Finding) -> Option<&'static str>` — check text+evidence against all protections
- [ ] `fn match_inflated_pattern(finding: &Finding) -> Option<&'static InflatedCategory>` — check text+evidence against all inflated categories
- [ ] `fn compute_new_severity(current: Severity, quantum: i32) -> Severity` — apply downgrade, clamped
- [ ] `fn apply_severity_auditor(findings: Vec<Finding>) -> Vec<Finding>` — full pipeline (NEVER_DOWNGRADE → multi-agent Critical → INFLATED_PATTERNS → downgrade)
- [ ] `fn format_severity_audit_report(before: &[Finding], after: &[Finding]) -> String` — human-readable report

## Harness Integration
- [ ] Wire `aggregate_batch()` call into evaluation pipeline in `main.rs` (replaces Python `aggregate_findings.py` subprocess call)
- [ ] Wire `apply_severity_auditor()` call into evaluation pipeline in `main.rs` (runs after aggregation, before final output)
- [ ] Tag each ported function as a library dependency — no CLI flags needed (library-only)
- [ ] Ensure `Finding` / `Candidate` / `Severity` types in `findings.rs` are compatible with both modules (add `severity_audited: bool`, `severity_audit_reason: Option<String>` to `Finding` if not present)
- [ ] Add `aggregator_stats` and `severity_auditor_stats` to output metadata

## Testing (Unit Tests)

### `aggregator.rs` Tests
- [ ] `test_classify_severity_exact` — all 4 canonical values
- [ ] `test_classify_severity_abbreviations` — "crit", "med"
- [ ] `test_classify_severity_prefix_match` — "crit...", "hig...", "med...", "low..."
- [ ] `test_classify_severity_default` — empty string, gibberish
- [ ] `test_normalize_basic` — lowercase, collapse whitespace
- [ ] `test_normalize_markdown` — strip `*_`#[]`` ``
- [ ] `test_extract_function` — "function foo", "method `bar`", "in `file.py#func`", "`my_func`", "file.func"
- [ ] `test_extract_function_none` — no match
- [ ] `test_jaccard_similarity_identical` — 1.0
- [ ] `test_jaccard_similarity_disjoint` — 0.0
- [ ] `test_jaccard_similarity_partial` — 0.5, 0.33 etc.
- [ ] `test_jaccard_similarity_empty` — 0.0 for empty/one-empty
- [ ] `test_semantic_dedup_single` — single finding passes through
- [ ] `test_semantic_dedup_same_function` — merges 2 findings in same file+func
- [ ] `test_semantic_dedup_same_line_bucket` — merges findings on adjacent lines
- [ ] `test_semantic_dedup_jaccard` — dedup by text similarity at 0.4 threshold
- [ ] `test_semantic_dedup_keeps_richest` — longest text, has line, has evidence
- [ ] `test_semantic_dedup_cross_validation` — combines agent counts
- [ ] `test_parse_report_table_format` — valid table report → findings
- [ ] `test_parse_report_bullet_format` — valid bullet report → findings
- [ ] `test_parse_report_json_format` — valid JSON report → findings
- [ ] `test_parse_report_empty` — empty/garbage text → empty vec
- [ ] `test_parse_report_stops_at_notes` — stops at "## Notes" section
- [ ] `test_format_candidate_basic` — badge + source
- [ ] `test_format_candidate_cross_validated` — [cross-validated] when cross_validated_by >= 2
- [ ] `test_aggregate_batch_single_pr` — full pipeline end-to-end

### `severity_auditor.rs` Tests
- [ ] `test_severity_value` — Critical=0, High=1, Medium=2, Low=3
- [ ] `test_has_never_downgrade_security` — SQL injection, XSS, RCE patterns found
- [ ] `test_has_never_downgrade_data_integrity` — race condition, deadlock patterns found
- [ ] `test_has_never_downgrade_correctness` — null pointer, crash patterns found
- [ ] `test_has_never_downgrade_no_match` — style nit text → None
- [ ] `test_has_never_downgrade_in_evidence` — pattern in evidence, not text
- [ ] `test_match_inflated_architecture` — SRP violation, God class → architecture_nits
- [ ] `test_match_inflated_hypothetical` — "could cause" → hypothetical_theoretical
- [ ] `test_match_inflated_style` — naming convention, formatting → style_nits
- [ ] `test_match_inflated_no_match` — genuine bug text → None
- [ ] `test_apply_severity_auditor_protected` — SQL injection finding not downgraded
- [ ] `test_apply_severity_auditor_multi_agent_critical` — Critical with 3 agents → not downgraded
- [ ] `test_apply_severity_auditor_architecture_downgrade` — HIGH SRP → LOW (-2)
- [ ] `test_apply_severity_auditor_hypothetical_downgrade` — HIGH "could cause" → MEDIUM (-1)
- [ ] `test_apply_severity_auditor_style_downgrade` — HIGH naming convention → LOW (-3)
- [ ] `test_apply_severity_auditor_no_downgrade_above_critical` — never goes above Critical
- [ ] `test_apply_severity_auditor_no_upgrade` — Low stays Low
- [ ] `test_apply_severity_auditor_trail_fields` — severity_audited, severity_audit_reason set
- [ ] `test_format_severity_audit_report_basic` — valid report string generated

### Integration Tests
- [ ] Port full self-test from `severity_auditor.py` `_run_self_test()` — all 7 test cases with expected results
- [ ] Parse one real Phase 4 report (table format) end-to-end through both modules
- [ ] Parse one real Phase 4 report (bullet format) end-to-end
- [ ] Verify no-panic on malformed input (truncated tables, missing fields, garbage text)
