//! Tests for `ReviewParams`, `ReviewMode`, and public helpers.

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// ReviewParams
// ---------------------------------------------------------------------------

#[test]
fn review_params_default_roles_when_empty() {
    let params = crb_harness::ReviewParams {
        diff: "some diff".to_string(),
        model: "test-model".to_string(),
        pr_title: "Test PR".to_string(),
        roles: vec![],
        max_findings: 20,
        replay_dir: None,
        cache_dir: None,
    };
    assert_eq!(params.diff, "some diff");
    assert_eq!(params.model, "test-model");
    assert!(params.roles.is_empty());
    assert!(params.replay_dir.is_none());
    assert!(params.cache_dir.is_none());
}

#[test]
fn review_params_custom_roles() {
    let params = crb_harness::ReviewParams {
        diff: String::new(),
        model: "m".to_string(),
        pr_title: "t".to_string(),
        roles: vec!["SA".to_string(), "SEC".to_string()],
        max_findings: 10,
        replay_dir: None,
        cache_dir: None,
    };
    assert_eq!(params.roles.len(), 2);
    assert_eq!(params.max_findings, 10);
}

// ---------------------------------------------------------------------------
// ReviewMode
// ---------------------------------------------------------------------------

#[test]
fn review_mode_commits() {
    let mode = crb_harness::ReviewMode::Commits {
        base: "HEAD~3".to_string(),
        head: "HEAD".to_string(),
    };
    match mode {
        crb_harness::ReviewMode::Commits { base, head } => {
            assert_eq!(base, "HEAD~3");
            assert_eq!(head, "HEAD");
        }
        _ => panic!("Expected Commits variant"),
    }
}

#[test]
fn review_mode_working() {
    let mode = crb_harness::ReviewMode::Working;
    match mode {
        crb_harness::ReviewMode::Working => {} // ok
        _ => panic!("Expected Working variant"),
    }
}

// ---------------------------------------------------------------------------
// ReviewArgs (from config)
// ---------------------------------------------------------------------------

#[test]
fn review_args_default_path_is_dot() {
    use clap::Parser;
    // Simulate `--working` with no path specified
    let args = crb_harness::config::ReviewArgs::parse_from(["test", "--working"]);
    assert_eq!(args.path, PathBuf::from("."));
    assert!(args.working);
    assert!(args.commits.is_none());
}

#[test]
fn review_args_commit_range() {
    use clap::Parser;
    let args =
        crb_harness::config::ReviewArgs::parse_from(["test", "--commits", "HEAD~3..HEAD"]);
    assert_eq!(args.commits.as_deref(), Some("HEAD~3..HEAD"));
    assert!(!args.working);
}

#[test]
fn review_args_custom_path() {
    use clap::Parser;
    let args = crb_harness::config::ReviewArgs::parse_from([
        "test",
        "--working",
        "--path",
        "/some/repo",
        "--model",
        "gpt-4o",
    ]);
    assert_eq!(args.path, PathBuf::from("/some/repo"));
    assert_eq!(args.model, "gpt-4o");
}

// ---------------------------------------------------------------------------
// load_cached_diff
// ---------------------------------------------------------------------------

#[test]
fn load_cached_diff_nonexistent() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let result = crb_harness::load_cached_diff(dir.path(), "owner", "repo", 42);
    assert!(result.is_none());
}

#[test]
fn load_cached_diff_exists() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let diffs_dir = dir.path().join("diffs");
    std::fs::create_dir_all(&diffs_dir).expect("create diffs dir");
    let diff_path = diffs_dir.join("owner_repo_42.diff");
    std::fs::write(&diff_path, "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new").expect("write diff");
    let result = crb_harness::load_cached_diff(dir.path(), "owner", "repo", 42);
    assert!(result.is_some());
    let content = result.unwrap();
    assert!(content.contains("old"));
    assert!(content.contains("new"));
}

// ---------------------------------------------------------------------------
// trim_end_matches on URL (tested via extract_pr_info but also independently)
// ---------------------------------------------------------------------------

#[test]
fn sanitize_filename_via_utils() {
    use crb_harness::utils;
    assert_eq!(utils::sanitize_filename("hello world"), "hello_world");
    assert_eq!(utils::sanitize_filename("file.name.txt"), "file_name_txt");
    assert_eq!(utils::sanitize_filename("already_ok"), "already_ok");
    assert_eq!(utils::sanitize_filename(""), "");
    assert_eq!(utils::sanitize_filename("a|b<c>d:e"), "a_b_c_d_e");
}

// ---------------------------------------------------------------------------
// Cli parsing (only Review variant)
// ---------------------------------------------------------------------------

#[test]
fn cli_review_subcommand() {
    use clap::Parser;
    let cli =
        crb_harness::config::Cli::parse_from(["crb-harness", "review", "--working"]);
    match cli {
        crb_harness::config::Cli::Review(args) => {
            assert!(args.working);
        }
    }
}
