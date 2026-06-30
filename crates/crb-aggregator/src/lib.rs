//! Port of `aggregate_findings.py` - report parsing, deduplication, and candidate formatting.
//!
//! Provides the core aggregation logic for code-review findings, including
//! multi-format report parsing, semantic deduplication, and candidate formatting.

pub use crb_agents::Finding;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TOOL_NAME: &str = "hermes";

/// Maximum number of candidates to emit per PR.
pub const MAX_CANDIDATES_PER_PR: usize = 20;

/// Minimum number of agents required to award the [cross-validated] badge.
pub const CROSS_VALIDATED_BADGE_THRESHOLD: usize = 2;

/// Severity ordering: lower number = more severe.
pub const SEVERITY_ORDER: &[(&str, u8)] = &[
    ("critical", 0),
    ("high", 1),
    ("medium", 2),
    ("low", 3),
];

/// Lookup helper: get severity rank by name.
pub fn severity_rank(severity: &str) -> u8 {
    for (name, rank) in SEVERITY_ORDER {
        if *name == severity {
            return *rank;
        }
    }
    2 // default medium
}

// ---------------------------------------------------------------------------
// Severity enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Critical => "critical",
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
        }
    }

    pub fn rank(&self) -> u8 {
        match self {
            Severity::Critical => 0,
            Severity::High => 1,
            Severity::Medium => 2,
            Severity::Low => 3,
        }
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A formatted candidate ready for output.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub text: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub source: String,
}

/// Aggregate batch statistics.
#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub total_findings: usize,
    pub candidates: usize,
    pub parse_warnings: Vec<String>,
    pub reports_with_warnings: Vec<String>,
    pub passed_to_adjudication: usize,
    pub report_stats: HashMap<String, ReportStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportStats {
    pub findings_count: usize,
    pub parse_warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Regex patterns (compiled once)
// ---------------------------------------------------------------------------

/// Match severity section headers: ### Critical Findings
static SEV_SECTION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^###\s*(Critical|High|Medium|Low)\s*(Findings|Issues|Vulnerabilities)s?\s*$")
        .unwrap()
});

/// Match finding heading: ### 🔴 C1 - Title
static HEADING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^###\s+\S\s+([CHML]\d+)\s*[-–—]\s*(.+)$")
        .unwrap()
});

/// Match table row: | **Field** | Value |
static TABLE_ROW_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\|\s*\*\*([^*]+)\*\*\s*\|\s*(.+?)\s*\|")
        .unwrap()
});

/// Match header row (skip)
static HEADER_ROW_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\|\s*\*\*(?:Field|Key)\*\*\s*\|\s*\*\*(?:Value|Description)\*\*\s*\|")
        .unwrap()
});

/// Extract path from backtick wrapping: `path/to/file`
static PATH_BACKTICK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"`([^`]+)`")
        .unwrap()
});

/// Bullet with ID: - **C1**: description
static BULLET_ID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[-*]\s+\*\*([CHML]\d+)\*\*\s*[:–—]\s*(.+)$")
        .unwrap()
});

/// Bullet with severity: - **CRITICAL**: description
static BULLET_SEV_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^[-*]\s+\*\*(Critical|High|Medium|Low)\*\*\s*[:–—]\s*(.+)$")
        .unwrap()
});

/// Prose format: **CRITICAL**: description
static PROSE_SEV_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\*\*(Critical|High|Medium|Low)\*\*\s*[:–—]\s*(.+)$")
        .unwrap()
});

/// Agent identifier pattern for counting.
static AGENT_ID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(SA|CL|AR|SEC)\b")
        .unwrap()
});

/// Pre-existing / notes section stopper.
static NOTES_STOPPER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^##\s+(pre-existing|Notes)")
        .unwrap()
});

/// JSON block detection (array or object).
static JSON_BLOCK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\[[\s\S]*\]|\{[\s\S]*\})")
        .unwrap()
});

// ---------------------------------------------------------------------------
// Helper: classify_severity
// ---------------------------------------------------------------------------

/// Normalise a severity string to one of: critical, high, medium, low.
pub fn classify_severity(text: &str) -> String {
    let cleaned = text.trim().to_lowercase().replace(' ', "_");
    match cleaned.as_str() {
        "critical" | "crit" => return "critical".to_string(),
        "high" => return "high".to_string(),
        "medium" | "med" => return "medium".to_string(),
        "low" => return "low".to_string(),
        _ => {}
    }
    // Prefix match fallback
    for (prefix, normal) in &[("crit", "critical"), ("high", "high"), ("med", "medium"), ("low", "low")] {
        if cleaned.starts_with(prefix) {
            return normal.to_string();
        }
    }
    "medium".to_string()
}

// ---------------------------------------------------------------------------
// Helper: normalize
// ---------------------------------------------------------------------------

/// Lowercase, strip markdown formatting and collapse whitespace.
pub fn normalize(text: &str) -> String {
    let text = text.to_lowercase();
    let text = MARKDOWN_CHARS_RE.replace_all(&text, "");
    let text = WHITESPACE_RE.replace_all(&text, " ");
    text.trim().to_string()
}

static MARKDOWN_CHARS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[*_`#\[\]]").unwrap());
static WHITESPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());

// ---------------------------------------------------------------------------
// Helper: extract_function
// ---------------------------------------------------------------------------

/// Try to extract a function/method name from finding text.
pub fn extract_function(text: &str) -> Option<String> {
    // Pattern 1: function/method/class/def/const `name`
    let pats: &[Regex] = &[
        Regex::new(r"(?i)(?:function|method|class|def|const)\s+`?(\w+)`?").unwrap(),
        Regex::new(r"(?i)`?([\w.]+)`?\s*(?:function|method|class)").unwrap(),
        Regex::new(r"(?i)(?:in|at|from|within)\s+`?([\w.:]+)[`#](\w+)`?").unwrap(),
        Regex::new(r"`([\w.]+)`").unwrap(),
    ];

    for pat in pats {
        if let Some(caps) = pat.captures(text) {
            if caps.len() >= 3 && caps.get(2).map_or(false, |m| !m.as_str().is_empty()) {
                return Some(format!("{}.{}", &caps[1], &caps[2]));
            }
            return Some(caps[1].to_string());
        }
    }

    // Fallback: `name#method` or `name.method`
    let fallback = Regex::new(r"(\w+)[#.](\w+)").unwrap();
    if let Some(caps) = fallback.captures(text) {
        return Some(format!("{}.{}", &caps[1], &caps[2]));
    }

    None
}

// ---------------------------------------------------------------------------
// Helper: jaccard_similarity
// ---------------------------------------------------------------------------

/// Compute Jaccard similarity between two text strings.
pub fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let norm_a = normalize(a);
    let norm_b = normalize(b);
    let words_a: HashSet<&str> = norm_a.split_whitespace().collect();
    let words_b: HashSet<&str> = norm_b.split_whitespace().collect();
    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }
    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

// ---------------------------------------------------------------------------
// Semantic dedup
// ---------------------------------------------------------------------------

/// Deduplicate findings by (file, function) then by text similarity.
pub fn semantic_dedup(findings: Vec<Map<String, Value>>) -> Vec<Map<String, Value>> {
    if findings.len() <= 1 {
        return findings;
    }

    let mut groups: HashMap<(String, String), Vec<Map<String, Value>>> = HashMap::new();
    let mut ungrouped: Vec<Map<String, Value>> = Vec::new();

    for f in findings {
        let file = f.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let func = f.get("text").and_then(|v| v.as_str()).map(extract_function).flatten();
        let line = f.get("line").and_then(|v| v.as_u64()).unwrap_or(0);

        if let Some(fn_name) = func {
            if !file.is_empty() {
                groups.entry((file.clone(), fn_name)).or_default().push(f);
                continue;
            }
        }
        if !file.is_empty() {
            let bucket = format!("inline_{}", line / 10);
            groups.entry((file, bucket)).or_default().push(f);
        } else {
            ungrouped.push(f);
        }
    }

    let mut merged: Vec<Map<String, Value>> = Vec::new();

    for (_key, group) in groups {
        if group.len() == 1 {
            merged.push(group.into_iter().next().unwrap());
        } else {
            // Merge: keep richest finding, track cross-validation
            let best = group
                .iter()
                .max_by(|a, b| {
                    let a_score = score_finding(a);
                    let b_score = score_finding(b);
                    a_score.cmp(&b_score)
                })
                .unwrap()
                .clone();

            // Combine agent counts
            let total_agents: u64 = group
                .iter()
                .map(|f| {
                    f.get("cross_validated_by")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1)
                })
                .sum();

            let mut result = best;
            result.insert("cross_validated".to_string(), Value::Bool(true));
            result.insert("cross_validated_by".to_string(), Value::Number(total_agents.into()));
            result.insert("merged_from".to_string(), Value::Number(group.len().into()));
            merged.push(result);
        }
    }

    // Ungrouped similarity merge
    if ungrouped.len() > 1 {
        let mut merged_indices: HashSet<usize> = HashSet::new();
        let sim_threshold = 0.4;

        for i in 0..ungrouped.len() {
            if merged_indices.contains(&i) {
                continue;
            }
            let mut similar: Vec<usize> = vec![i];
            for j in (i + 1)..ungrouped.len() {
                if merged_indices.contains(&j) {
                    continue;
                }
                let sim = jaccard_similarity(
                    ungrouped[i].get("text").and_then(|v| v.as_str()).unwrap_or(""),
                    ungrouped[j].get("text").and_then(|v| v.as_str()).unwrap_or(""),
                );
                if sim >= sim_threshold {
                    similar.push(j);
                    merged_indices.insert(j);
                }
            }
            if similar.len() > 1 {
                merged_indices.insert(i);
                let best = similar
                    .iter()
                    .max_by(|&&a, &&b| {
                        ungrouped[a]
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .len()
                            .cmp(
                                &ungrouped[b]
                                    .get("text")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .len(),
                            )
                    })
                    .unwrap();
                let mut best_finding = ungrouped[*best].clone();
                let total_agents: u64 = similar
                    .iter()
                    .map(|&idx| {
                        ungrouped[idx]
                            .get("cross_validated_by")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(1)
                    })
                    .sum();
                best_finding.insert("cross_validated".to_string(), Value::Bool(true));
                best_finding.insert(
                    "cross_validated_by".to_string(),
                    Value::Number(total_agents.into()),
                );
                merged.push(best_finding);
            } else if !merged_indices.contains(&i) {
                merged.push(ungrouped[i].clone());
            }
        }
    } else if ungrouped.len() == 1 {
        merged.push(ungrouped.into_iter().next().unwrap());
    }

    merged
}

fn score_finding(f: &Map<String, Value>) -> (usize, bool, bool) {
    let text_len = f
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.len())
        .unwrap_or(0);
    let has_line = f.get("line").is_some();
    let has_evidence = f
        .get("evidence")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    (text_len, has_line, has_evidence)
}

// ---------------------------------------------------------------------------
// Candidate formatting
// ---------------------------------------------------------------------------

/// Format a finding dictionary into a Candidate struct.
pub fn format_candidate(finding: &Map<String, Value>, as_notes: bool) -> Candidate {
    let sev = finding
        .get("severity")
        .and_then(|v| v.as_str())
        .unwrap_or("Medium");
    let text = finding
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let agent_count = finding
        .get("cross_validated_by")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let cross = if agent_count >= CROSS_VALIDATED_BADGE_THRESHOLD as u64 {
        " [cross-validated]"
    } else {
        ""
    };
    let prefix = if as_notes { "[NOTES]" } else { "" };

    let formatted_text = if text.is_empty() {
        text.to_string()
    } else {
        format!("{prefix}[{sev}]{cross} {text}", sev = capitalize(sev))
    };

    Candidate {
        text: formatted_text,
        path: finding.get("path").and_then(|v| v.as_str()).map(|s| s.to_string()),
        line: finding.get("line").and_then(|v| v.as_u64()),
        source: "orchestrator_phase4".to_string(),
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

// ---------------------------------------------------------------------------
// Report parsing
// ---------------------------------------------------------------------------

/// Parse Phase 4 report text with multi-format support.
///
/// Tries strategies in order (stop at first that yields findings):
///   a) Table-row format (original Phase 4 format)
///   b) Bullet-list / prose format
///   c) JSON fallback
pub fn parse_report(report_text: &str, parse_warnings: &mut Vec<String>) -> Vec<Map<String, Value>> {
    let findings = _parse_table_format(report_text, parse_warnings);
    if !findings.is_empty() {
        return findings;
    }

    let findings = _parse_bullet_format(report_text, parse_warnings);
    if !findings.is_empty() {
        return findings;
    }

    let findings = _parse_json_format(report_text, parse_warnings);
    if !findings.is_empty() {
        return findings;
    }

    vec![]
}

fn _parse_table_format(report_text: &str, parse_warnings: &mut Vec<String>) -> Vec<Map<String, Value>> {
    let mut findings: Vec<Map<String, Value>> = vec![];
    let mut current: Option<Map<String, Value>> = None;

    for line in report_text.lines() {
        let line = line.trim();

        // Stop at pre-existing or notes section
        if NOTES_STOPPER_RE.is_match(line) {
            break;
        }

        // Parse heading: ### 🔴 C1 - Title
        if let Some(caps) = HEADING_RE.captures(line) {
            if let Some(ref cur) = current {
                if cur.get("text").and_then(|v| v.as_str()).map_or(false, |t| !t.is_empty()) {
                    findings.push(cur.clone());
                }
            }
            let mut new_finding = Map::new();
            new_finding.insert("id".to_string(), Value::String(caps[1].to_string()));
            new_finding.insert("text".to_string(), Value::String(caps[2].trim().to_string()));
            current = Some(new_finding);
            continue;
        }

        if current.is_none() {
            continue;
        }

        // Skip header rows
        if HEADER_ROW_RE.is_match(line) {
            continue;
        }

        // Parse table row: | **Field** | Value |
        if let Some(caps) = TABLE_ROW_RE.captures(line) {
            let raw_field = caps[1].trim();
            let value = caps[2].trim().replace("**", "");
            let field_lower = raw_field.to_lowercase().replace(' ', "_");

            if let Some(ref mut cur) = current {
                if field_lower == "severity" || field_lower.starts_with("severity") {
                    cur.insert("severity".to_string(), Value::String(value));
                } else if field_lower.starts_with("file") {
                    if let Some(path_caps) = PATH_BACKTICK_RE.captures(&value) {
                        cur.insert("path".to_string(), Value::String(path_caps[1].to_string()));
                    } else {
                        let path = value.split(',').next().unwrap_or(&value).trim().to_string();
                        cur.insert("path".to_string(), Value::String(path));
                    }
                } else if field_lower == "line" || field_lower.starts_with("line") {
                    let num_str: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
                    if let Ok(n) = num_str.parse::<u64>() {
                        cur.insert("line".to_string(), Value::Number(n.into()));
                    }
                } else if field_lower == "description" || field_lower.starts_with("description") {
                    let existing = cur
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let new_text = if existing.is_empty() {
                        value.clone()
                    } else {
                        format!("{}: {}", existing, value)
                    };
                    cur.insert("text".to_string(), Value::String(new_text));
                } else if field_lower.starts_with("found") {
                    cur.insert("found_by".to_string(), Value::String(value.clone()));
                    let agent_count = AGENT_ID_RE.find_iter(&value).count();
                    cur.insert("cross_validated".to_string(), Value::Bool(agent_count >= 1));
                    cur.insert("cross_validated_by".to_string(), Value::Number(agent_count.into()));
                } else if field_lower == "evidence" || field_lower.starts_with("evidence") {
                    cur.insert("evidence".to_string(), Value::String(value));
                } else if field_lower == "confidence" || field_lower.starts_with("confidence") {
                    cur.insert("confidence".to_string(), Value::String(value));
                }
            }
        }
    }

    // Flush last finding
    if let Some(cur) = current {
        if cur.get("text").and_then(|v| v.as_str()).map_or(false, |t| !t.is_empty()) {
            findings.push(cur);
        }
    }

    // Check for partial matches
    for f in &findings {
        let fid = f.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        if !f.contains_key("severity") {
            parse_warnings.push(format!("Finding {} missing severity field", fid));
        }
        if !f.contains_key("path") {
            parse_warnings.push(format!("Finding {} missing path/file field", fid));
        }
    }

    findings
}

fn _parse_bullet_format(report_text: &str, parse_warnings: &mut Vec<String>) -> Vec<Map<String, Value>> {
    let mut findings: Vec<Map<String, Value>> = vec![];
    let mut section_severity: Option<String> = None;

    for line in report_text.lines() {
        let line = line.trim();

        // Check for severity section headers
        if let Some(caps) = SEV_SECTION_RE.captures(line) {
            section_severity = Some(caps[1].to_uppercase());
            continue;
        }

        // Bullet with ID: - **C1**: description
        if let Some(caps) = BULLET_ID_RE.captures(line) {
            let mut finding = Map::new();
            finding.insert("id".to_string(), Value::String(caps[1].to_string()));
            finding.insert("text".to_string(), Value::String(caps[2].trim().to_string()));
            finding.insert(
                "severity".to_string(),
                Value::String(section_severity.clone().unwrap_or_else(|| "Medium".to_string())),
            );
            finding.insert("cross_validated".to_string(), Value::Bool(true));
            finding.insert("cross_validated_by".to_string(), Value::Number(1.into()));
            findings.push(finding);
            continue;
        }

        // Bullet with severity: - **CRITICAL**: description
        if let Some(caps) = BULLET_SEV_RE.captures(line) {
            let mut finding = Map::new();
            finding.insert(
                "id".to_string(),
                Value::String(format!("X{}", findings.len() + 1)),
            );
            finding.insert("text".to_string(), Value::String(caps[2].trim().to_string()));
            finding.insert("severity".to_string(), Value::String(caps[1].to_uppercase()));
            finding.insert("cross_validated".to_string(), Value::Bool(true));
            finding.insert("cross_validated_by".to_string(), Value::Number(1.into()));
            findings.push(finding);
            continue;
        }

        // Prose format: **CRITICAL**: description
        if let Some(caps) = PROSE_SEV_RE.captures(line) {
            let mut finding = Map::new();
            finding.insert(
                "id".to_string(),
                Value::String(format!("P{}", findings.len() + 1)),
            );
            finding.insert("text".to_string(), Value::String(caps[2].trim().to_string()));
            finding.insert("severity".to_string(), Value::String(caps[1].to_uppercase()));
            finding.insert("cross_validated".to_string(), Value::Bool(true));
            finding.insert("cross_validated_by".to_string(), Value::Number(1.into()));
            findings.push(finding);
        }
    }

    if !findings.is_empty() {
        parse_warnings.push(format!(
            "Recovered {} findings via bullet-list/prose format",
            findings.len()
        ));
    }

    findings
}

fn _parse_json_format(report_text: &str, parse_warnings: &mut Vec<String>) -> Vec<Map<String, Value>> {
    if !report_text.contains('{') || !report_text.contains('}') {
        return vec![];
    }

    // Try to parse as JSON directly
    let data: Value = match serde_json::from_str(report_text) {
        Ok(v) => v,
        Err(_) => {
            // Try to find JSON block within markdown
            if let Some(caps) = JSON_BLOCK_RE.captures(report_text) {
                match serde_json::from_str(caps.get(1).unwrap().as_str()) {
                    Ok(v) => v,
                    Err(_) => return vec![],
                }
            } else {
                return vec![];
            }
        }
    };

    let items: Vec<Value> = match &data {
        Value::Array(arr) => arr.clone(),
        Value::Object(map) => {
            // Try known keys
            for key in &["findings", "results", "vulnerabilities", "issues"] {
                if let Some(val) = map.get(*key) {
                    if let Some(arr) = val.as_array() {
                        return _extract_json_findings(arr, parse_warnings);
                    }
                }
            }
            return vec![];
        }
        _ => return vec![],
    };

    _extract_json_findings(&items, parse_warnings)
}

fn _extract_json_findings(items: &[Value], parse_warnings: &mut Vec<String>) -> Vec<Map<String, Value>> {
    let mut findings: Vec<Map<String, Value>> = vec![];

    for item in items {
        if let Some(obj) = item.as_object() {
            let mut finding = Map::new();
            finding.insert(
                "id".to_string(),
                Value::String(
                    obj.get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("J{}", findings.len() + 1)),
                ),
            );
            finding.insert(
                "text".to_string(),
                Value::String(
                    obj.get("text")
                        .or_else(|| obj.get("description"))
                        .or_else(|| obj.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                ),
            );
            finding.insert(
                "severity".to_string(),
                Value::String(
                    obj.get("severity")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Medium")
                        .to_uppercase(),
                ),
            );
            if let Some(path) = obj
                .get("path")
                .or_else(|| obj.get("file"))
                .and_then(|v| v.as_str())
            {
                finding.insert("path".to_string(), Value::String(path.to_string()));
            }
            if let Some(line_val) = obj.get("line").and_then(|v| v.as_u64()) {
                finding.insert("line".to_string(), Value::Number(line_val.into()));
            }
            finding.insert("cross_validated".to_string(), Value::Bool(true));
            finding.insert(
                "cross_validated_by".to_string(),
                Value::Number(
                    obj.get("cross_validated_by")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1)
                        .into(),
                ),
            );

            if finding
                .get("text")
                .and_then(|v| v.as_str())
                .map_or(false, |t| !t.is_empty())
            {
                findings.push(finding);
            }
        }
    }

    if !findings.is_empty() {
        parse_warnings.push(format!(
            "Recovered {} findings via JSON fallback",
            findings.len()
        ));
    }

    findings
}

// ---------------------------------------------------------------------------
// Raw report archiving (keeping interface, logic is a simple file write)
// ---------------------------------------------------------------------------

/// Save each report's raw text to disk before parsing.
pub fn archive_raw_reports(pr_reports: &HashMap<String, String>, archive_dir: &str) {
    let path = std::path::Path::new(archive_dir);
    let _ = std::fs::create_dir_all(path);

    for (pr_key, report_text) in pr_reports {
        let safe_key: String = pr_key
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let dest = path.join(format!("{}_report.md", safe_key));
        let _ = std::fs::write(&dest, report_text);
    }
}

// ---------------------------------------------------------------------------
// Aggregate batch
// ---------------------------------------------------------------------------

/// Phase 4.6 aggregator - v5 flat pass-through (no hybrid gate).
///
/// All deduped findings go directly to the output. No severity-based routing,
/// no secondary review queue, no auto-promotion.
pub fn aggregate_batch(
    pr_reports: HashMap<String, String>,
    max_per_pr: usize,
    archive_reports: bool,
) -> (HashMap<String, HashMap<String, Vec<Map<String, Value>>>>, Stats) {
    // Archive raw reports (optional)
    if archive_reports {
        let default_archive = "/data/workspace/projects/code-review-benchmark-research/datasets/code-review-benchmark/offline/hermes_data/raw_reports/phase4.5/";
        archive_raw_reports(&pr_reports, default_archive);
    }

    let mut candidates: HashMap<String, HashMap<String, Vec<Map<String, Value>>>> = HashMap::new();
    let mut stats = Stats {
        total_findings: 0,
        candidates: 0,
        parse_warnings: vec![],
        reports_with_warnings: vec![],
        passed_to_adjudication: 0,
        report_stats: HashMap::new(),
    };

    for (pr_url, report) in &pr_reports {
        let pr_key = pr_url.to_string();
        let mut pr_warnings: Vec<String> = vec![];

        let findings = parse_report(report, &mut pr_warnings);

        stats.report_stats.insert(
            pr_key.clone(),
            ReportStats {
                findings_count: findings.len(),
                parse_warnings: pr_warnings.clone(),
            },
        );

        if !pr_warnings.is_empty() {
            stats.parse_warnings.extend(pr_warnings.clone());
            stats.reports_with_warnings.push(pr_key.clone());
        }

        if findings.is_empty() {
            let mut empty_map = HashMap::new();
            empty_map.insert(TOOL_NAME.to_string(), vec![]);
            candidates.insert(pr_url.clone(), empty_map);
            continue;
        }

        stats.total_findings += findings.len();
        let deduped = semantic_dedup(findings);

        // V5 flat pass-through - all deduped findings go to output
        let mut candidates_for_pr: Vec<Map<String, Value>> = deduped;

        stats.passed_to_adjudication += candidates_for_pr.len();

        // Severity-weighted capping
        candidates_for_pr.sort_by_key(|f| {
            let sev = f
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("medium");
            severity_rank(&classify_severity(sev))
        });

        if candidates_for_pr.len() > max_per_pr {
            candidates_for_pr.truncate(max_per_pr);
        }

        stats.candidates += candidates_for_pr.len();

        let cand_list: Vec<Map<String, Value>> = candidates_for_pr
            .iter()
            .map(|f| {
                let candidate = format_candidate(f, false);
                let mut map = Map::new();
                map.insert("text".to_string(), Value::String(candidate.text));
                map.insert(
                    "path".to_string(),
                    match candidate.path {
                        Some(p) => Value::String(p),
                        None => Value::Null,
                    },
                );
                map.insert(
                    "line".to_string(),
                    match candidate.line {
                        Some(l) => Value::Number(l.into()),
                        None => Value::Null,
                    },
                );
                map.insert("source".to_string(), Value::String(candidate.source));
                map
            })
            .collect();

        let mut pr_map = HashMap::new();
        pr_map.insert(TOOL_NAME.to_string(), cand_list);
        candidates.insert(pr_url.clone(), pr_map);
    }

    (candidates, stats)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_severity() {
        assert_eq!(classify_severity("CRITICAL"), "critical");
        assert_eq!(classify_severity("crit"), "critical");
        assert_eq!(classify_severity("High"), "high");
        assert_eq!(classify_severity("MED"), "medium");
        assert_eq!(classify_severity("Low"), "low");
        assert_eq!(classify_severity(""), "medium");
        assert_eq!(classify_severity("unknown"), "medium");
    }

    #[test]
    fn test_normalize() {
        let n = normalize(" **CRITICAL**: This is a *test* ");
        assert!(!n.contains('*'));
        assert!(!n.contains('#'));
        assert_eq!(n, "critical: this is a test");
    }

    #[test]
    fn test_extract_function() {
        let cases = vec![
            ("function foo()", Some("foo")),
            ("def my_func():", Some("my_func")),
            ("class MyClass:", Some("MyClass")),
            ("no matching text here", None),
        ];
        for (input, expected) in cases {
            let result = extract_function(input);
            assert_eq!(result.as_deref(), expected, "input: {}", input);
        }
    }

    #[test]
    fn test_jaccard_similarity() {
        let a = "hello world foo bar";
        let b = "hello world baz qux";
        let sim = jaccard_similarity(a, b);
        assert!(sim > 0.0);
        assert!(sim < 1.0);
        assert_eq!(jaccard_similarity("hello", "world"), 0.0);
        assert_eq!(jaccard_similarity("same same", "same same"), 1.0);
        assert_eq!(jaccard_similarity("", "hello"), 0.0);
    }

    #[test]
    fn test_semantic_dedup_empty() {
        let result = semantic_dedup(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_semantic_dedup_single() {
        let mut f = Map::new();
        f.insert("text".to_string(), Value::String("test".to_string()));
        let result = semantic_dedup(vec![f.clone()]);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_format_candidate() {
        let mut f = Map::new();
        f.insert("severity".to_string(), Value::String("high".to_string()));
        f.insert("text".to_string(), Value::String("a bug".to_string()));
        f.insert("path".to_string(), Value::String("src/main.rs".to_string()));
        f.insert("line".to_string(), Value::Number(42.into()));
        f.insert("cross_validated_by".to_string(), Value::Number(1.into()));

        let c = format_candidate(&f, false);
        assert_eq!(c.text, "[High] a bug");
        assert_eq!(c.path.unwrap(), "src/main.rs");
        assert_eq!(c.line.unwrap(), 42);
    }

    #[test]
    fn test_format_candidate_cross_validated() {
        let mut f = Map::new();
        f.insert("severity".to_string(), Value::String("critical".to_string()));
        f.insert("text".to_string(), Value::String("real bug".to_string()));
        f.insert("cross_validated_by".to_string(), Value::Number(3.into()));

        let c = format_candidate(&f, false);
        assert!(c.text.contains("[cross-validated]"));
    }

    #[test]
    fn test_format_candidate_as_notes() {
        let mut f = Map::new();
        f.insert("severity".to_string(), Value::String("low".to_string()));
        f.insert("text".to_string(), Value::String("nit".to_string()));
        f.insert("cross_validated_by".to_string(), Value::Number(1.into()));

        let c = format_candidate(&f, true);
        assert!(c.text.starts_with("[NOTES]"));
    }

    #[test]
    fn test_parse_table_format() {
        let report = "\
### 🔴 C1 — Security issue
| **Field** | **Value** |
| **Severity** | High |
| **File** | `src/auth.rs` |
| **Line** | 42 |
| **Description** | Missing input validation |
| **Evidence** | Raw user input used |
| **Found by** | SA, CL |
";
        let mut warnings = vec![];
        let findings = _parse_table_format(report, &mut warnings);
        assert!(!findings.is_empty(), "should parse table format");
        let f = &findings[0];
        assert_eq!(
            f.get("text").and_then(|v| v.as_str()).unwrap_or(""),
            "Security issue: Missing input validation"
        );
        assert_eq!(
            f.get("severity").and_then(|v| v.as_str()).unwrap_or(""),
            "High"
        );
        assert_eq!(
            f.get("path").and_then(|v| v.as_str()).unwrap_or(""),
            "src/auth.rs"
        );
        assert_eq!(f.get("line").and_then(|v| v.as_u64()).unwrap_or(0), 42);
    }

    #[test]
    fn test_parse_bullet_format() {
        let report = "\
### Critical Findings
- **C1**: SQL injection vulnerability
- **C2**: XSS in user input
";
        let mut warnings = vec![];
        let findings = _parse_bullet_format(report, &mut warnings);
        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings[0].get("text").and_then(|v| v.as_str()).unwrap_or(""),
            "SQL injection vulnerability"
        );
        assert_eq!(
            findings[0].get("severity").and_then(|v| v.as_str()).unwrap_or(""),
            "CRITICAL"
        );
    }

    #[test]
    fn test_parse_json_format() {
        let report = r#"[
            {"text": "Bug one", "severity": "high", "path": "src/lib.rs", "line": 10}
        ]"#;
        let mut warnings = vec![];
        let findings = _parse_json_format(report, &mut warnings);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].get("text").and_then(|v| v.as_str()).unwrap_or(""),
            "Bug one"
        );
    }

    #[test]
    fn test_parse_report_tries_formats() {
        // Should succeed with table format
        let table = "\
### 🔴 H1 — Test finding
| **Field** | **Value** |
| **Severity** | Medium |
| **File** | `test.rs` |
| **Description** | A test finding |
";
        let mut warnings = vec![];
        let findings = parse_report(table, &mut warnings);
        assert!(!findings.is_empty());

        // Should fall back to JSON
        let json = r#"[{"text": "JSON finding", "severity": "low"}]"#;
        let mut warnings2 = vec![];
        let findings2 = parse_report(json, &mut warnings2);
        assert!(!findings2.is_empty());
    }

    #[test]
    fn test_aggregate_batch_empty() {
        let reports = HashMap::new();
        let (cands, stats) = aggregate_batch(reports, MAX_CANDIDATES_PER_PR, false);
        assert!(cands.is_empty());
        assert_eq!(stats.total_findings, 0);
    }

    #[test]
    fn test_aggregate_batch_single() {
        let mut reports = HashMap::new();
        let report = "\
### 🔴 H1 — Test
| **Severity** | High |
| **File** | `main.rs` |
| **Description** | A bug
";
        reports.insert("test/repo/1".to_string(), report.to_string());
        let (cands, stats) = aggregate_batch(reports, MAX_CANDIDATES_PER_PR, false);
        assert_eq!(cands.len(), 1);
        assert!(stats.total_findings >= 1);
    }

    #[test]
    fn test_severity_rank() {
        assert_eq!(severity_rank("critical"), 0);
        assert_eq!(severity_rank("high"), 1);
        assert_eq!(severity_rank("medium"), 2);
        assert_eq!(severity_rank("low"), 3);
        assert_eq!(severity_rank("unknown"), 2);
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("hello"), "Hello");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("h"), "H");
    }
}
