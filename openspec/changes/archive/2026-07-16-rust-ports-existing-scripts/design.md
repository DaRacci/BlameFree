# Design: Rust Ports of `aggregate_findings.py` + `severity_auditor.py`

## Architecture

Both modules live as separate workspace crates with dual lib.rs+main.rs. The library exposes pure functions; the CLI wraps those functions with clap argument parsing.

```
review-harness/
├── Cargo.toml                     # [workspace] members = ["crates/*"]
└── crates/
    ├── crb-aggregator/             # Port of aggregate_findings.py
    │   ├── Cargo.toml              # deps: serde, serde_json, regex, crb-agents
    │   └── src/
    │       ├── lib.rs              # Pure library: parse_report, semantic_dedup, aggregate_batch, etc.
    │       └── main.rs             # Standalone CLI: --reports-dir, --output, --replace, --pr-filter
    ├── crb-auditor/                # Port of severity_auditor.py
    │   ├── Cargo.toml              # deps: serde, serde_json, regex, crb-agents
    │   └── src/
    │       ├── lib.rs              # Pure library: apply_severity_auditor, format_report, patterns
    │       └── main.rs             # Standalone CLI: --findings-file, --output, --report
    └── crb-agents/                 # Shared types
        ├── Cargo.toml              # deps: serde, schemars
        └── src/lib.rs              # Finding, Severity, Candidate types
```

### Data Flow

```
report text (String)
    │
    ▼
aggregator::parse_report()             ── tries 3 strategies in order:
    │                                      (a) table-row format (regex)
    │                                      (b) bullet-list/prose format
    │                                      (c) JSON fallback (serde_json)
    ▼
Vec<Finding>
    │
    ▼
aggregator::semantic_dedup()           ── group by (file,function) then
    │                                      (file, line_bucket), keep richest;
    │                                      Jaccard dedup at 0.4 threshold
    ▼
Vec<Finding> (deduped)
    │
    ▼
severity_auditor::apply_severity_auditor()  ── NEVER_DOWNGRADE check ->
    │                                              multi-agent Critical check ->
    ▼                                             INFLATED_PATTERNS match -> downgrade
Vec<Finding> (audited)
    │
    ▼
aggregator::aggregate_batch()          ── sort by severity, cap at 20,
    │                                      format_candidate on each
    ▼
Map<Url, Candidates> + Stats
```

## Module: `aggregator.rs`

### Functions

#### `fn classify_severity(text: &str) -> Severity`

Normalise a severity string to the `Severity` enum. Handles:
- Direct matches: `"critical"` / `"crit"` -> `Severity::Critical`, `"high"` -> `Severity::High`, `"medium"` / `"med"` -> `Severity::Medium`, `"low"` -> `Severity::Low`. Case-insensitive.
- Prefix matching ("crit" -> Critical, "hig" -> High, "med" -> Medium, "low" -> Low).
- Default to `Severity::Medium` on empty or unrecognised input.

#### `fn normalize(text: &str) -> String`

- Lowercase the text.
- Strip markdown formatting: `*`, `_`, `` ` ``, `#`, `[`, `]`.
- Collapse whitespace (multiple spaces -> single space, trim).

#### `fn extract_function(text: &str) -> Option<String>`

Apply a series of regex patterns (in order) to extract a function/method/class name:
1. `` (?:function|method|class|def|const)\s+`?(\w+)`? ``
2. `` `?([\w.]+)`?\s*(?:function|method|class) ``
3. `` (?:in|at|from|within)\s+`?([\w.:]+)[`#](\w+)`? `` — returns `"file.func"` format
4. `` `([\w.]+)` ``
5. Fallback: `r'(\w+)[#.](\w+)'` — returns `"file.func"`

Returns `None` if no match found.

#### `fn jaccard_similarity(a: &str, b: &str) -> f64`

- Normalize both texts via `normalize()`.
- Split into words, build `HashSet<&str>` for each (zero alloc per word).
- `intersection / union`. Returns `0.0` if either set is empty.

#### `fn semantic_dedup(findings: Vec<Finding>) -> Vec<Finding>`

Three-tier dedup pipeline:
1. **Group by (file, function)** — findings in the same file with the same extracted function name. Keep the richest finding (longest text, has line, has evidence). Combine cross-validation counts across all members.
2. **Group by (file, line_bucket)** — for findings with a file but no function name, group by `file` + `line // 10` (adjacent lines in same file). Same merge logic.
3. **Jaccard similarity** — for remaining ungrouped findings, dedup at similarity threshold `>= 0.4`. Keep richest per cluster.

Returns deduped findings. If input has `<= 1` finding, returns as-is.

#### `fn format_candidate(finding: &Finding) -> Candidate`

Build a `Candidate` struct with:
- `text`: `"[{Severity}][cross-validated] {text}"` — cross-validated badge if `cross_validated_by >= 2`.
- `path`, `line` from the finding.
- `source`: `"orchestrator_phase4"`.

#### `fn parse_report(report_text: &str) -> Result<Vec<Finding>>`

Multi-format parser with 3 strategies tried in order (stop at first that yields findings):

**(a) Table-row format** (`_parse_table_format`):
- Parse `### 🔴 C1 — Title` headings for finding ID and text.
- Parse `| **Field** | Value |` rows for severity, file, line, description, found_by, evidence, confidence.
- Skip `| **Field** | **Value** |` header rows.
- Stop at `## pre-existing` or `## Notes` sections.
- Handle file fields with/without backtick wrapping, agent count via `re.findall(r'\b(SA|CL|AR|SEC)\b', ...)`.

**(b) Bullet-list / prose format** (`_parse_bullet_format`):
- Severity section headers: `### CRITICAL Findings`.
- Bullet items: `- **C1**: description` or `- **CRITICAL**: description`.
- Prose lines: `**CRITICAL**: description`.

**(c) JSON fallback** (`_parse_json_format`):
- Try `serde_json::from_str` directly.
- If that fails, extract JSON array/object from within markdown via regex.
- Handle both `[...]` arrays and `{"findings": [...]}` objects.
- Support `text`, `description`, `title`, `path`, `file`, `line` field name variants.

Returns empty `Vec` if no findings parsed.

#### `fn aggregate_batch(pr_reports: Map<Url, Report>) -> (Map<Url, Candidates>, Stats)`

Full pipeline per PR:
1. Parse each report via `parse_report()`.
2. Dedup via `semantic_dedup()`.
3. Sort by severity order (Critical->Low).
4. Cap at `MAX_CANDIDATES_PER_PR = 20`.
5. Format via `format_candidate()`.

Returns:
- `Map<Url, Candidates>` — mapping PR URL -> tool-name -> candidate list
- `Stats` — `{ total_findings, candidates, parse_warnings, reports_with_warnings, passed_to_adjudication, report_stats }`

### Key Types (shared, defined in `crb-agents` crate)

See `crates/crb-agents/src/lib.rs` for `Finding`, `Candidate`, and `Severity` type definitions, now shared across all workspace crates.

### Regex Compilation

Use `LazyLock<Vec<Regex>>` for pattern lists that are compiled once at module init:
- Extract-function patterns: 5 regexes compiled once.
- Table-row heading pattern: one `LazyLock<Regex>`.
- Table-row field pattern: one `Lazy<Regex>`.
- Agent-code pattern: one `Lazy<Regex>`.
- Bullet/prose patterns: one `Lazy<Regex>` per variant.

## Module: `severity_auditor.rs`

### Data Structures

```rust
pub struct InflatedCategory {
    pub name: &'static str,
    pub description: &'static str,
    pub patterns: Vec<Regex>,
    pub downgrade_quantum: i32,  // negative: -2, -1, -3
}

pub struct ProtectionCategory {
    pub name: &'static str,
    pub patterns: Vec<Regex>,
}
```

Static instances (compiled once via `LazyLock`):
- `INFLATED_CATEGORIES: LazyLock<Vec<InflatedCategory>>` — 3 categories (architecture_nits, hypothetical_theoretical, style_nits).
- `NEVER_DOWNGRADE_CATEGORIES: LazyLock<Vec<ProtectionCategory>>` — 3 categories (security_vulns, data_integrity, correctness_bugs).

### Functions

#### `fn severity_value(severity: &Severity) -> u8`

Maps `Critical -> 0, High -> 1, Medium -> 2, Low -> 3`.

#### `fn has_never_downgrade_pattern(finding: &Finding) -> Option<&'static str>`

Check finding `text + evidence` (lowercased, combined) against all `NEVER_DOWNGRADE_CATEGORIES` patterns. Returns `Some(category_name)` on first match, `None` otherwise.

#### `fn has_inflated_pattern(finding: &Finding) -> Option<&'static InflatedCategory>`

Check combined text+evidence against all `INFLATED_CATEGORIES` patterns. Returns the first-matching category.

#### `fn compute_new_severity(current: Severity, quantum: i32) -> Severity`

Apply quantum: new_val = (current_val - quantum), clamped to `[Critical, Low]`. Since quantum is negative, this is a downgrade (higher numeric value = less severe).

#### `fn apply_severity_auditor(findings: Vec<Finding>) -> Vec<Finding>`

For each finding:
1. **NEVER_DOWNGRADE check** — if `has_never_downgrade_pattern()` returns `Some`, keep original severity. Set `severity_audited = false`, `severity_audit_reason = "protected_by_never_downgrade_pattern: {category}"`.
2. **Multi-agent Critical guard** — if severity is `Critical` and `cross_validated_by >= 2`, skip downgrade. Reason: `"protected_by_multi_agent_critical: {n}_agents"`.
3. **Inflated pattern check** — if `has_inflated_pattern()` returns a category, compute new severity via `compute_new_severity()`. Only downgrade (never upgrade). Reason: `"downgraded: {orig}->{new} by category='{cat}' pattern='{pat}' (quantum={q})"`.
4. If no inflated pattern matches: keep original, reason: `"no_inflated_patterns_matched"`.

Never upgrades. Never downgrades a finding past `Low`.

#### `fn format_severity_audit_report(before: &[Finding], after: &[Finding]) -> String`

Generate human-readable report:
- Total findings checked / after.
- Count downgraded (breakdown by category with percentages).
- Count protected (never-downgrade matched).
- Count skipped (multi-agent critical).
- Preservation rate.
- Sample first 5 downgrades with reason.

### Regex Compilation

- `INFLATED_CATEGORIES`: ~30 regexes total (arch: ~15, hypothethical: ~12, style: ~8). Compiled once at module init.
- `NEVER_DOWNGRADE_CATEGORIES`: ~25 regexes total (sec: ~18, data: ~6, correctness: ~10). Compiled once.
- No per-call regex compilation overhead.

### Pattern Reuse (between modules)

`aggregator.rs` and `severity_auditor.rs` both need `classify_severity()` / `severity_value()`. The `Severity` enum lives in `findings.rs`. The classification function can live in `aggregator.rs` and be re-exported, or in a shared `util.rs` — decision deferred to implementation.

## Key Decisions

|| Decision | Choice | Rationale |
||----------|--------|-----------|
|| Language | Rust | Same language as harness; zero overhead integration; no subprocess |
|| Crate structure | Separate crates (crb-aggregator, crb-auditor) | Independently publishable, testable, and usable as library or CLI |
|| Regex crate | `regex` | Standard, well-optimised; compiled patterns via `LazyLock` |
|| JSON parsing | `serde_json` | Already a dependency; matches existing `Finding` serde derives |
|| Module boundary | Pure functions, no I/O | File I/O stays in main.rs; modules are testable in isolation |
|| Dedup strategy | Group-key then Jaccard | Matches Python exactly; O(n²) Jaccard only for remaining ungrouped |
|| Severity type | `enum Severity` in `crb-agents` | Strongly-typed, cross-crate shared; no magic strings |
|| Compile-once patterns | `LazyLock<Vec<Regex>>` | Pattern list grows rarely; negligible memory cost |
|| parse_report order | table -> bullet -> JSON | Matches Python; table format is the primary Phase 4 format |
|| NEVER_DOWNGRADE priority | Highest | Security/integrity/correctness patterns must always win |
