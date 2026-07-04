//! Tests for diff extraction functions in crb-benchmark.
//!
//! Covers: creating a temp git repo, making commits, and verifying
//! diff content matches expectations.

use std::path::PathBuf;
use std::process::Command;

/// Helper: create a temp git repo with two commits to produce a meaningful diff.
fn setup_repo_with_diffs() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let repo_path = dir.path().to_path_buf();

    Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "diff@test.com"])
        .current_dir(&repo_path)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Diff Test"])
        .current_dir(&repo_path)
        .output()
        .expect("git config name");

    std::fs::write(
        repo_path.join("main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .expect("write");
    Command::new("git")
        .args(["add", "main.rs"])
        .current_dir(&repo_path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_path)
        .output()
        .expect("git commit");

    std::fs::write(
        repo_path.join("main.rs"),
        "fn main() {\n    println!(\"hello world\");\n    // added comment\n}\n",
    )
    .expect("write");
    Command::new("git")
        .args(["add", "main.rs"])
        .current_dir(&repo_path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "Update message"])
        .current_dir(&repo_path)
        .output()
        .expect("git commit");

    (dir, repo_path)
}

#[test]
fn git_diff_between_commits() {
    let (_dir, repo_path) = setup_repo_with_diffs();

    let output = Command::new("git")
        .args(["diff", "HEAD~1..HEAD"])
        .current_dir(&repo_path)
        .output()
        .expect("git diff");
    assert!(output.status.success(), "git diff should succeed");

    let diff = String::from_utf8_lossy(&output.stdout);
    assert!(!diff.is_empty(), "diff should not be empty");
    assert!(diff.contains("main.rs"), "diff should mention main.rs");
    assert!(
        diff.contains("hello world"),
        "diff should contain new content"
    );
    assert!(
        diff.contains("+") && diff.contains("-"),
        "diff should have additions and deletions"
    );
}

#[test]
fn git_diff_working_tree() {
    let (_dir, repo_path) = setup_repo_with_diffs();

    std::fs::write(
        repo_path.join("main.rs"),
        "fn main() {\n    println!(\"modified!\");\n}\n",
    )
    .expect("write");

    let output = Command::new("git")
        .arg("diff")
        .current_dir(&repo_path)
        .output()
        .expect("git diff");
    assert!(output.status.success());

    let diff = String::from_utf8_lossy(&output.stdout);
    assert!(!diff.is_empty(), "working tree diff should not be empty");
    assert!(
        diff.contains("modified"),
        "diff should show unstaged changes"
    );
}

#[test]
fn git_diff_staged_changes() {
    let (_dir, repo_path) = setup_repo_with_diffs();

    std::fs::write(repo_path.join("main.rs"), "fn main() {\n    // staged\n}\n").expect("write");
    Command::new("git")
        .args(["add", "main.rs"])
        .current_dir(&repo_path)
        .output()
        .expect("git add");

    let output = Command::new("git")
        .args(["diff", "--cached"])
        .current_dir(&repo_path)
        .output()
        .expect("git diff --cached");
    assert!(output.status.success());

    let diff = String::from_utf8_lossy(&output.stdout);
    assert!(!diff.is_empty(), "staged diff should not be empty");
    assert!(diff.contains("staged"), "diff should show staged content");
}

#[test]
fn git_diff_format_has_hunks() {
    let (_dir, repo_path) = setup_repo_with_diffs();

    let output = Command::new("git")
        .args(["diff", "HEAD~1..HEAD"])
        .current_dir(&repo_path)
        .output()
        .expect("git diff");
    let diff = String::from_utf8_lossy(&output.stdout);

    assert!(
        diff.starts_with("diff --git"),
        "diff should start with diff --git header"
    );
    assert!(diff.contains("@@"), "diff should contain hunk header (@@)");
}

#[test]
fn fetch_single_diff_via_worktree() {
    let (_dir, repo_path) = setup_repo_with_diffs();

    let worktree_dir = tempfile::TempDir::new().expect("worktree temp");
    let wt_path = worktree_dir.path().join("wt");

    let status = Command::new("git")
        .args(["worktree", "add", &wt_path.to_string_lossy(), "HEAD"])
        .current_dir(&repo_path)
        .status()
        .expect("git worktree add");
    assert!(status.success(), "worktree add");

    let output = Command::new("git")
        .args(["diff", "HEAD^", "HEAD"])
        .current_dir(&wt_path)
        .output()
        .expect("git diff in worktree");

    assert!(output.status.success(), "git diff should succeed");
    let diff = String::from_utf8_lossy(&output.stdout);
    assert!(!diff.is_empty(), "worktree diff should not be empty");
    assert!(
        diff.contains("hello world"),
        "diff should contain second commit content"
    );

    Command::new("git")
        .args(["worktree", "remove", "--force", &wt_path.to_string_lossy()])
        .current_dir(&repo_path)
        .status()
        .expect("git worktree remove");
}

#[test]
fn git_diff_on_empty_initial_commit() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let repo_path = dir.path().to_path_buf();

    Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "empty@test.com"])
        .current_dir(&repo_path)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Empty Test"])
        .current_dir(&repo_path)
        .output()
        .expect("git config name");

    Command::new("git")
        .args(["commit", "--allow-empty", "-m", "Initial"])
        .current_dir(&repo_path)
        .output()
        .expect("git commit");

    let output = Command::new("git")
        .arg("diff")
        .current_dir(&repo_path)
        .output()
        .expect("git diff on empty");
    assert!(output.status.success());
    let diff = String::from_utf8_lossy(&output.stdout);
    assert!(diff.is_empty(), "empty repo should have no diff");
}
