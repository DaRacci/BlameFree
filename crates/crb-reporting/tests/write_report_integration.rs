use std::{collections::HashMap, fs};

use crb_reporting::cost::{AnalyticsSnapshot, CacheUsage, SessionUsage};
use crb_reporting::write_report;
use crb_types::benchmark::golden::GoldenComment;
use crb_types::benchmark::judge::JudgeVerdict;
use crb_types::benchmark::metrics::Metrics;
use crb_types::benchmark::result::{JudgedFinding, PrResult};
use crb_types::vcs::pr::PrMeta;
use mti::prelude::{MagicTypeIdExt, V7};

fn make_cost_snapshot() -> AnalyticsSnapshot {
    let id = "agent".create_type_id::<V7>();
    let mut sessions = HashMap::new();
    sessions.insert(
        id.clone(),
        SessionUsage {
            input_tokens: 100,
            output_tokens: 50,
            cached_input_tokens: 20,
            cache_creation_input_tokens: 10,
            reasoning_tokens: 5,
            tool_use_prompt_tokens: 3,
            call_count: 1,
            tool_use_count: 0,
        },
    );
    let mut cache_usage = HashMap::new();
    cache_usage.insert(
        id.clone(),
        CacheUsage {
            cache_hits: 1,
            cache_misses: 0,
        },
    );
    AnalyticsSnapshot {
        sessions,
        cache_usage,
    }
}

fn make_pr(pr_title: &str, url: &str, has_cost: bool) -> PrResult {
    PrResult {
        meta: PrMeta {
            title: pr_title.to_string(),
            url: url.to_string(),
            number: todo!(),
        },
        // findings_count: 3,
        // golden_count: 2,
        metrics: Metrics {
            true_positives: 2,
            false_positives: 1,
            false_negatives: 0,
            duration_secs: 12.5,
        },
        findings_with_verdicts: vec![
            JudgedFinding {
                finding: todo!(),
                verdict: JudgeVerdict {
                    reasoning: "Match found".into(),
                    match_: true,
                    confidence: 0.95,
                },
            },
            JudgedFinding {
                finding: todo!(),
                verdict: JudgeVerdict {
                    reasoning: "No match".into(),
                    match_: false,
                    confidence: 0.1,
                },
            },
        ],
        golden_comments: vec![
            GoldenComment {
                comment: "This is a golden comment".into(),
                severity: crb_types::severity::Severity::Info,
            },
            GoldenComment {
                comment: "This is a medium severity".into(),
                severity: crb_types::severity::Severity::Medium,
            },
        ],
    }
}

#[test]
fn test_write_report_creates_per_pr_files() {
    let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
    let results = vec![
        make_pr("PR One", "https://github.com/a/b/pull/1", true),
        make_pr("PR Two", "https://github.com/a/b/pull/2", false),
    ];

    let result = write_report(&results, dir.path());
    assert!(result.is_ok(), "write_report should succeed");

    // Collect files in the output dir
    let mut files: Vec<String> = fs::read_dir(dir.path())
        .expect("read_dir should succeed")
        .filter_map(|e| {
            e.ok()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
        })
        .collect();
    files.sort();

    assert_eq!(files.len(), 2, "should create exactly 2 files");
    assert!(
        files.iter().any(|f| f == "PR_One.json"),
        "should have PR_One.json, got: {files:?}"
    );
    assert!(
        files.iter().any(|f| f == "PR_Two.json"),
        "should have PR_Two.json, got: {files:?}"
    );

    // Verify content of PR_One.json
    let content = fs::read_to_string(dir.path().join("PR_One.json"))
        .expect("read PR_One.json should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("parse PR_One.json should succeed");
    assert_eq!(parsed["pr_title"], "PR One");
    assert!(parsed["cost"].is_object());

    // Verify content of PR_Two.json
    let content2 = fs::read_to_string(dir.path().join("PR_Two.json"))
        .expect("read PR_Two.json should succeed");
    let parsed2: serde_json::Value =
        serde_json::from_str(&content2).expect("parse PR_Two.json should succeed");
    assert_eq!(parsed2["pr_title"], "PR Two");
    // Cost field should be absent (None)
    assert!(!parsed2.as_object().unwrap().contains_key("cost"));
}

#[test]
fn test_write_report_output_dir_created() {
    let base = tempfile::TempDir::new().expect("tempdir creation should succeed");
    let sub_path = base.path().join("sub").join("output");

    let results = vec![make_pr("Deep Path", "https://github.com/a/b/pull/3", true)];
    let result = write_report(&results, &sub_path);
    assert!(
        result.is_ok(),
        "write_report should create nested directory"
    );

    assert!(sub_path.exists(), "sub/output directory should exist");
    let file_path = sub_path.join("Deep_Path.json");
    assert!(
        file_path.exists(),
        "Deep_Path.json should exist in sub/output"
    );
}

#[test]
fn test_write_report_empty_slice() {
    let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
    let results: Vec<PrResult> = Vec::new();

    let result = write_report(&results, dir.path());
    assert!(
        result.is_ok(),
        "write_report with empty slice should succeed"
    );

    // Verify no files were created
    let count = fs::read_dir(dir.path())
        .expect("read_dir should succeed")
        .count();
    assert_eq!(count, 0, "no files should be created for empty slice");
}
