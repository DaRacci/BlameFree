//! Tests for scaffold-related functions in crb-benchmark.
//!
//! Covers: `parse_github_url()` with valid/invalid URLs,
//! and basic git worktree creation.

use std::path::PathBuf;
use std::process::Command;

// ---------------------------------------------------------------------------
// parse_github_url
// ---------------------------------------------------------------------------

#[test]
fn parse_github_url_valid() {
    let result = crb_benchmark::scaffold::parse_github_url("https://github.com/owner/repo/pull/42");
    assert!(result.is_ok());
    let (owner, repo, num) = result.unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(repo, "repo");
    assert_eq!(num, 42);
}

#[test]
fn parse_github_url_with_hyphens() {
    let result =
        crb_benchmark::scaffold::parse_github_url("https://github.com/my-org/my-repo/pull/123");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), ("my-org".to_string(), "my-repo".to_string(), 123));
}

#[test]
fn parse_github_url_not_github() {
    let result =
        crb_benchmark::scaffold::parse_github_url("https://gitlab.com/owner/repo/merge_requests/1");
    assert!(result.is_err());
}

#[test]
fn parse_github_url_no_pr_number() {
    let result = crb_benchmark::scaffold::parse_github_url("https://github.com/owner/repo");
    assert!(result.is_err());
}

#[test]
fn parse_github_url_empty() {
    let result = crb_benchmark::scaffold::parse_github_url("");
    assert!(result.is_err());
}

#[test]
fn parse_github_url_non_numeric_pr() {
    let result =
        crb_benchmark::scaffold::parse_github_url("https://github.com/owner/repo/pull/abc");
    assert!(result.is_err());
}

#[test]
fn parse_github_url_trailing_slash() {
    // The regex is not $ anchored, so trailing slash still matches
    let result =
        crb_benchmark::scaffold::parse_github_url("https://github.com/owner/repo/pull/42/");
    assert!(result.is_ok());
    let (owner, repo, num) = result.unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(repo, "repo");
    assert_eq!(num, 42);
}

// ---------------------------------------------------------------------------
// Worktree creation (basic git operations)
// ---------------------------------------------------------------------------

/// Helper: create a temp git repo with a commit.
fn setup_git_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let repo_path = dir.path().to_path_buf();

    Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "wt@test.com"])
        .current_dir(&repo_path)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Worktree Test"])
        .current_dir(&repo_path)
        .output()
        .expect("git config name");

    // Create a file and commit
    std::fs::write(repo_path.join("test.txt"), "initial content").expect("write");
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(&repo_path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "Initial"])
        .current_dir(&repo_path)
        .output()
        .expect("git commit");

    (dir, repo_path)
}

#[test]
fn worktree_add_and_remove() {
    let (_dir, repo_path) = setup_git_repo();
    let worktree_dir = tempfile::TempDir::new().expect("worktree temp dir");
    let wt_path = worktree_dir.path().join("wt");

    // Add worktree
    let status = Command::new("git")
        .args(["worktree", "add", &wt_path.to_string_lossy(), "HEAD"])
        .current_dir(&repo_path)
        .status()
        .expect("git worktree add");
    assert!(status.success(), "worktree add should succeed");

    // Verify worktree exists as a git-linked directory
    assert!(wt_path.join(".git").exists() || wt_path.join(".git").is_file());
    assert!(wt_path.join("test.txt").exists());

    // Verify content matches
    let content = std::fs::read_to_string(wt_path.join("test.txt")).expect("read wt file");
    assert_eq!(content, "initial content");

    // Remove worktree
    let status = Command::new("git")
        .args(["worktree", "remove", "--force", &wt_path.to_string_lossy()])
        .current_dir(&repo_path)
        .status()
        .expect("git worktree remove");
    assert!(status.success(), "worktree remove should succeed");

    // Prune
    Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(&repo_path)
        .status()
        .expect("git worktree prune");
}

// ---------------------------------------------------------------------------
// Scaffold directory structure
// ---------------------------------------------------------------------------

#[test]
fn scaffold_dir_structure() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let benchmark_dir = dir.path().join("benchmark");

    // Create expected scaffold structure
    let base_repos = benchmark_dir.join("base-repos");
    let diffs = benchmark_dir.join("diffs");
    let worktrees = benchmark_dir.join("worktrees");

    std::fs::create_dir_all(&base_repos).expect("create base-repos");
    std::fs::create_dir_all(&diffs).expect("create diffs");
    std::fs::create_dir_all(&worktrees).expect("create worktrees");

    assert!(base_repos.exists());
    assert!(diffs.exists());
    assert!(worktrees.exists());
}
