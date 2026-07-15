//! Tests for `ReviewParams`, `ReviewMode`, and public helpers.

use crb_agents::prompts::PromptLibrary;

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
        cache_dir: None,
    };
    assert_eq!(params.diff, "some diff");
    assert_eq!(params.model, "test-model");
    assert!(params.roles.is_empty());
    assert!(params.cache_dir.is_none());
}

#[test]
fn review_params_custom_roles() {
    let params = crb_harness::ReviewParams {
        diff: String::new(),
        model: "m".to_string(),
        pr_title: "t".to_string(),
        roles: PromptLibrary::get_instance()
            .abbreviations()
            .into_iter()
            .take(2)
            .map(|s| s.to_string())
            .collect(),
        max_findings: 10,
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
    std::fs::write(
        &diff_path,
        "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new",
    )
    .expect("write diff");
    let result = crb_harness::load_cached_diff(dir.path(), "owner", "repo", 42);
    assert!(result.is_some());
    let content = result.unwrap();
    assert!(content.contains("old"));
    assert!(content.contains("new"));
}
