use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde_json::{Map, Value};

use crate::jaccard_similarity;

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

/// Deduplicate findings by (file, function) then by text similarity.
pub fn semantic_dedup(findings: Vec<Map<String, Value>>) -> Vec<Map<String, Value>> {
    if findings.len() <= 1 {
        return findings;
    }

    let mut groups: HashMap<(String, String), Vec<Map<String, Value>>> = HashMap::new();
    let mut ungrouped: Vec<Map<String, Value>> = Vec::new();

    for f in findings {
        let file = f
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let func = f
            .get("text")
            .and_then(|v| v.as_str())
            .map(extract_function)
            .flatten();
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
            result.insert(
                "cross_validated_by".to_string(),
                Value::Number(total_agents.into()),
            );
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
                    ungrouped[i]
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    ungrouped[j]
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    true,
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
        let mut f = Map::new();
        f.insert("text".to_string(), Value::String("test".to_string()));
        let result = semantic_dedup(vec![f.clone()]);
        assert_eq!(result.len(), 1);
    }
}
