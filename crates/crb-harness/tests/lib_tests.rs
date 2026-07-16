//! Tests for `ReviewMode` and public helpers.

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

#[test]
fn load_cached_diff_nonexistent() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let result = crb_benchmark::diff_cache::load_cached_diff(dir.path(), "owner", "repo", 42);
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
    let result = crb_benchmark::diff_cache::load_cached_diff(dir.path(), "owner", "repo", 42);
    assert!(result.is_some());
    let content = result.unwrap();
    assert!(content.contains("old"));
    assert!(content.contains("new"));
}
