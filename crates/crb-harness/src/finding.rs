use crb_auditor::apply_severity_auditor;
use crb_shared::{deduplicate::semantic_dedup, finding::Finding, severity::Severity};

const MAX_FINDINGS: usize = 20;
const MAX_RESPONSE_PREVIEW: usize = 500;
const MAX_RESPONSE_TRUNCATE: usize = 200;

/// Post-process findings through aggregator dedup and auditor severity checks.
pub fn post_process_findings(findings: &[Finding]) -> Vec<Finding> {
    if findings.is_empty() {
        return findings.to_vec();
    }

    let mut findings = semantic_dedup(findings.to_vec());
    apply_severity_auditor(&mut findings);
    let capped = {
        let max = MAX_FINDINGS;
        if findings.len() > max {
            info!("capping {} findings to {} candidates", findings.len(), max);
            findings.into_iter().take(max).collect()
        } else {
            findings
        }
    };

    capped
}

/// Parse an agents LLM response string into a `Vec<Finding>`.
///
/// Attempts three strategies in order:
/// 1. Direct JSON array deserialization via `serde_json::from_str`.
/// 2. JSON extraction from markdown fenced code blocks (```json ... ```).
/// 3. Find any JSON array in the response.
///
/// If all strategies fail, returns an empty `Vec`.
#[deprecated = "Use the `TypedPrompt` from rig instead of parsing raw LLM responses."]
pub fn parse_agent_findings(response: &str) -> Result<Vec<Finding>, String> {
    let preview_len = std::cmp::min(MAX_RESPONSE_PREVIEW, response.len());
    info!(
        "Agent raw response (first {} chars): {}",
        preview_len,
        &response[..preview_len]
    );

    // Try direct JSON array parse with normalisation
    if let Ok(findings) = serde_json::from_str(response) {
        info!(
            "Parsed {} finding(s) directly from agent JSON response",
            findings.len()
        );
        return Ok(findings);
    }

    // Extract JSON from markdown code blocks
    let re = Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n\s*```").unwrap();
    if let Some(caps) = re.captures(response) {
        let inner = caps.get(1).unwrap().as_str().trim();
        if let Ok(findings) = serde_json::from_str(inner) {
            info!(
                "Parsed {} finding(s) from markdown code block in agent response",
                findings.len()
            );
            return Ok(findings);
        }
    }

    // Find any JSON array in the response
    let array_re = Regex::new(r"\[[\s\S]*\]").unwrap();
    if let Some(m) = array_re.find(response) {
        if let Ok(findings) = serde_json::from_str(m) {
            info!(
                "Parsed {} finding(s) from embedded JSON array",
                findings.len()
            );
            return Ok(findings);
        }
    }

    let truncated = if response.len() > MAX_RESPONSE_TRUNCATE {
        format!("{}...", &response[..MAX_RESPONSE_TRUNCATE])
    } else {
        response.to_string()
    };
    warn!("Failed to parse agent response as Finding array.\nResponse (truncated): {truncated}",);
    Ok(Vec::new())
}

/// Deduplicate a list of findings by (file, line) pairs.
///
/// When two findings share the same file path and line number, only the first occurrence is kept.
/// This avoids double-counting findings that multiple agents or chunks produced for the same location.
///
/// # Ordering
///
/// The deduplication is stable: the first occurrence of each (file, line) pair
/// is retained, and subsequent duplicates are dropped.
pub fn deduplicate_findings(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen: HashSet<(String, u32)> = HashSet::new();
    let mut result = Vec::with_capacity(findings.len());

    for f in findings {
        let key = (f.file.clone().unwrap_or_default(), f.line.unwrap_or(0));
        if seen.insert(key) {
            result.push(f);
        }
    }

    result
}
