use std::collections::HashSet;
use tracing::info;

use crb_auditor::apply_severity_auditor;
use crb_shared::{deduplicate::semantic_dedup, finding::Finding};

const MAX_FINDINGS: usize = 20;

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
