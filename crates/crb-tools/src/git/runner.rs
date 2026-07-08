use std::{io, process::Command, time::Duration};

use crate::error::GitError;

/// Run a git subcommand in the given repository with a 60-second timeout.
///
/// Executes `git -C <repo_path> <args...>` via `spawn_blocking`, handles the
/// join error, the I/O error, the timeout, and extracts UTF-8 stdout on success
/// or returns a structured [`GitError`] on failure.
pub async fn run_git_command(repo_path: &str, args: &[&str]) -> Result<String, GitError> {
    let repo_path = repo_path.to_owned();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();

    let result = tokio::time::timeout(Duration::from_secs(60), async move {
        tokio::task::spawn_blocking(move || {
            Command::new("git")
                .arg("-C")
                .arg(&repo_path)
                .args(&args)
                .output()
        })
        .await
        .map_err(|join_err| GitError::CommandFailed(io::Error::other(join_err.to_string())))?
        .map_err(GitError::CommandFailed)
    })
    .await
    .map_err(|_| GitError::TimeoutElapsed)??;

    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).to_string())
    } else {
        Err(GitError::NonZeroExit(
            result.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&result.stderr).to_string(),
        ))
    }
}
