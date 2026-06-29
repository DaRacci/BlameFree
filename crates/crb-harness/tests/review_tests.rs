//! Tests for `review_diff()`.
//!
//! Tests that review_diff runs without panicking on a real git repo.
//! It should fail gracefully with "no API key" rather than panic.

use std::path::PathBuf;
use std::process::Command;

/// Helper: create a temporary git repo with a file and a commit.
fn setup_temp_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let repo_path = dir.path().to_path_buf();

    Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .output()
        .expect("git init");

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&repo_path)
        .output()
        .expect("git config email");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&repo_path)
        .output()
        .expect("git config name");

    // Create a file and commit it
    std::fs::write(repo_path.join("hello.txt"), "hello world").expect("write file");
    Command::new("git")
        .args(["add", "hello.txt"])
        .current_dir(&repo_path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_path)
        .output()
        .expect("git commit");

    (dir, repo_path)
}

// ---------------------------------------------------------------------------
// review_diff on working tree with no changes → empty findings
// ---------------------------------------------------------------------------

#[test]
fn review_diff_no_changes() {
    let (_dir, repo_path) = setup_temp_repo();

    let args = crb_harness::config::ReviewArgs {
        commits: None,
        working: true,
        path: repo_path,
        model: "dummy-model".to_string(),
    };

    // Should not panic — even without an API key, review_diff should
    // return Ok(Vec::new()) when there's no diff.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(crb_harness::review_diff(args))
    }));

    assert!(result.is_ok(), "review_diff should not panic");
    let findings = result.unwrap();
    assert!(findings.is_ok(), "review_diff should return Ok: {:?}", findings.err());
    assert!(findings.unwrap().is_empty(), "expected empty findings for clean repo");
}

// ---------------------------------------------------------------------------
// review_diff with unstaged changes → should produce a diff but fail
// gracefully without API key
// ---------------------------------------------------------------------------

#[test]
fn review_diff_with_unstaged_changes() {
    let (_dir, repo_path) = setup_temp_repo();

    // Make an unstaged change
    std::fs::write(repo_path.join("hello.txt"), "hello world modified").expect("write file");

    let args = crb_harness::config::ReviewArgs {
        commits: None,
        working: true,
        path: repo_path,
        model: "dummy-model".to_string(),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(crb_harness::review_diff(args))
    }));

    assert!(result.is_ok(), "review_diff should not panic with changes");
    // With no API key, review_pr will return an error about the client —
    // that's acceptable (graceful failure).
    let findings = result.unwrap();
    // Without an API key, client creation fails: "Failed to create OpenAI client: ..."
    // This is fine — we just want to ensure no panic.
    if let Err(e) = &findings {
        let msg = e.to_string();
        assert!(
            msg.contains("OpenAI client") || msg.contains("API key"),
            "Unexpected error: {msg}"
        );
    }
}

// ---------------------------------------------------------------------------
// review_diff with commit range
// ---------------------------------------------------------------------------

#[test]
fn review_diff_commit_range() {
    let (_dir, repo_path) = setup_temp_repo();

    let args = crb_harness::config::ReviewArgs {
        commits: Some("HEAD~1..HEAD".to_string()),
        working: false,
        path: repo_path,
        model: "dummy-model".to_string(),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(crb_harness::review_diff(args))
    }));

    assert!(result.is_ok(), "review_diff with commit range should not panic");
}

// ---------------------------------------------------------------------------
// review_diff with non-existent path (should fail gracefully)
// ---------------------------------------------------------------------------

#[test]
fn review_diff_bad_path() {
    let args = crb_harness::config::ReviewArgs {
        commits: None,
        working: true,
        path: PathBuf::from("/tmp/nonexistent-repo-12345"),
        model: "dummy-model".to_string(),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(crb_harness::review_diff(args))
    }));

    assert!(result.is_ok(), "review_diff with bad path should not panic");
    // Should fail with an error about git, not a panic
    let findings = result.unwrap();
    assert!(findings.is_err(), "expected error for non-git-repo path");
}
