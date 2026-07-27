use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Create a temp git repo with one file committed.
pub fn setup_temp_repo() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
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

/// Create a temp git repo with two commits producing a meaningful diff.
pub fn setup_repo_with_diffs() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
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

/// Create a temp git repo with one empty commit (no files).
pub fn setup_empty_commit_repo() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
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

    (dir, repo_path)
}
