use std::collections::{HashMap, HashSet};

use crb_types::finding::Finding;
use regex::Regex;

use crate::jaccard::jaccard_similarity;

/// Try to extract a function/method name from finding text.
#[allow(clippy::unwrap_used)]
pub(crate) fn extract_function(text: &str) -> Option<String> {
    let pats: &[Regex] = &[
        Regex::new(r"(?i)(?:function|method|class|def|const)\s+`?(\w+)`?").unwrap(),
        Regex::new(r"(?i)`?([\w.]+)`?\s*(?:function|method|class)").unwrap(),
        Regex::new(r"(?i)(?:in|at|from|within)\s+`?([\w.:]+)[`#](\w+)`?").unwrap(),
        Regex::new(r"`([\w.]+)`").unwrap(),
    ];

    for pat in pats {
        if let Some(caps) = pat.captures(text) {
            if caps.len() >= 3 && caps.get(2).is_some_and(|m| !m.as_str().is_empty()) {
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

/// Deduplicate findings by (file, function) then by text similarity.
#[allow(clippy::unwrap_used)]
pub fn semantic_dedup(findings: Vec<Finding>) -> Vec<Finding> {
    if findings.len() <= 1 {
        return findings;
    }

    let mut groups: HashMap<(String, String), Vec<Finding>> = HashMap::new();
    let mut ungrouped: Vec<Finding> = Vec::new();

    for f in findings {
        let file = f.file.clone().unwrap_or_default();
        let func = extract_function(&f.message);
        let line = f.line.unwrap_or(0);

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

    let mut merged: Vec<Finding> = Vec::new();

    for (_key, group) in groups {
        if group.len() == 1 {
            merged.push(group.into_iter().next().unwrap());
        } else {
            // Merge: keep richest finding, track cross-validation
            let mut best = group
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
                .map(|f| f.cross_validated_by.unwrap_or(1))
                .sum();

            best.cross_validated = true;
            best.cross_validated_by = Some(total_agents);
            best.merged_from = Some(group.len() as u64);
            merged.push(best);
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
                let sim = jaccard_similarity(&ungrouped[i].message, &ungrouped[j].message, true);
                if sim >= sim_threshold {
                    similar.push(j);
                    merged_indices.insert(j);
                }
            }
            if similar.len() > 1 {
                merged_indices.insert(i);
                let best_idx = *similar
                    .iter()
                    .max_by(|&&a, &&b| ungrouped[a].message.len().cmp(&ungrouped[b].message.len()))
                    .unwrap();
                let mut best_finding = ungrouped[best_idx].clone();
                let total_agents: u64 = similar
                    .iter()
                    .map(|&idx| ungrouped[idx].cross_validated_by.unwrap_or(1))
                    .sum();
                best_finding.cross_validated = true;
                best_finding.cross_validated_by = Some(total_agents);
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

fn score_finding(f: &Finding) -> (usize, bool, bool) {
    let text_len = f.message.len();
    let has_line = f.line.is_some();
    let has_evidence = f
        .evidence
        .as_deref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    (text_len, has_line, has_evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_semantic_dedup_empty() {
        let result = semantic_dedup(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_semantic_dedup_single() {
        let f = Finding {
            message: "test".to_string(),
            ..Default::default()
        };
        let result = semantic_dedup(vec![f]);
        assert_eq!(result.len(), 1);
    }
}
