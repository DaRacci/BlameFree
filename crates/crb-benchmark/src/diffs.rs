use std::path::Path;
use std::process::Command;

use anyhow::Result;
use tracing::info;

/// Extract diffs from all scaffolded repos.
pub fn run(repos_dir: &Path, output_dir: &Path) -> Result<()> {
    if !repos_dir.exists() {
        anyhow::bail!("Repos directory does not exist: {}", repos_dir.display());
    }

    std::fs::create_dir_all(output_dir)?;
    info!("Scanning repos in: {}", repos_dir.display());

    let mut diff_count = 0;
    for entry in std::fs::read_dir(repos_dir)? {
        let entry = entry?;
        let repo_path = entry.path();
        if !repo_path.is_dir() {
            continue;
        }
        if !repo_path.join(".git").exists() {
            tracing::warn!("Not a git repo, skipping: {}", repo_path.display());
            continue;
        }

        let repo_name = repo_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Get the diff between HEAD (PR merge) and its parent
        let output = Command::new("git")
            .args(["diff", "HEAD^", "HEAD"])
            .current_dir(&repo_path)
            .output()?;

        if output.status.success() {
            let diff_path = output_dir.join(format!("{repo_name}.diff"));
            std::fs::write(&diff_path, &output.stdout)?;
            let line_count = String::from_utf8_lossy(&output.stdout).lines().count();
            info!(
                "Extracted diff for {} ({} lines) -> {}",
                repo_name,
                line_count,
                diff_path.display()
            );
            diff_count += 1;
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // HEAD^ may fail if the checkout is not a merge commit — try diff with no parent
            if stderr.contains("ambiguous argument") || stderr.contains("unknown revision") {
                tracing::warn!(
                    "HEAD^ not available for {}, trying --root diff",
                    repo_name
                );
                let root_output = Command::new("git")
                    .args(["diff", "--root", "HEAD"])
                    .current_dir(&repo_path)
                    .output()?;
                if root_output.status.success() {
                    let diff_path = output_dir.join(format!("{repo_name}.diff"));
                    std::fs::write(&diff_path, &root_output.stdout)?;
                    let line_count =
                        String::from_utf8_lossy(&root_output.stdout).lines().count();
                    info!(
                        "Extracted root diff for {} ({} lines) -> {}",
                        repo_name,
                        line_count,
                        diff_path.display()
                    );
                    diff_count += 1;
                } else {
                    tracing::warn!("Failed to extract root diff for {}: {}", repo_name,
                        String::from_utf8_lossy(&root_output.stderr));
                }
            } else {
                tracing::warn!(
                    "Failed to extract diff for {}: {}",
                    repo_name,
                    stderr.trim()
                );
            }
        }
    }

    info!(
        "Fetch-diffs complete: {} diff(s) extracted to {}",
        diff_count,
        output_dir.display()
    );

    if diff_count == 0 {
        println!("No diffs extracted. Run `crb-benchmark scaffold` first.");
    }

    Ok(())
}

/// Fetch diff for a single PR (used by harness for per-PR setup).
///
/// Checks out the PR merge ref and extracts the diff against its parent.
/// Returns the diff text as a string.
pub fn fetch_single_diff(repo_path: &Path, pr_number: u32) -> Result<String> {
    // Fetch the PR merge ref
    let status = Command::new("git")
        .args([
            "fetch",
            "origin",
            &format!("pull/{pr_number}/merge"),
        ])
        .current_dir(repo_path)
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to fetch PR #{pr_number} merge ref from {}", repo_path.display());
    }

    // Checkout the merge commit (detached HEAD)
    let checkout_status = Command::new("git")
        .args(["checkout", "--detach", "FETCH_HEAD"])
        .current_dir(repo_path)
        .status()?;

    if !checkout_status.success() {
        anyhow::bail!("Failed to checkout FETCH_HEAD for PR #{pr_number}");
    }

    // Get the diff between the merge commit and its first parent (the base)
    let output = Command::new("git")
        .args(["diff", "HEAD^", "HEAD"])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        // Fallback: --root diff if HEAD^ doesn't exist
        let root_output = Command::new("git")
            .args(["diff", "--root", "HEAD"])
            .current_dir(repo_path)
            .output()?;
        if root_output.status.success() {
            return Ok(String::from_utf8_lossy(&root_output.stdout).to_string());
        }
        anyhow::bail!(
            "Failed to get diff for PR #{pr_number}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Checkout a PR merge commit in a scaffolded repo (used by harness).
pub fn checkout_pr(repo_path: &Path, pr_number: u32) -> Result<()> {
    // Fetch the PR merge ref
    let status = Command::new("git")
        .args([
            "fetch",
            "origin",
            &format!("pull/{pr_number}/merge"),
        ])
        .current_dir(repo_path)
        .status()?;

    if !status.success() {
        // Fallback: fetch PR head
        tracing::warn!(
            "PR #{pr_number} merge ref not available for {}, falling back to PR head",
            repo_path.display()
        );
        let status = Command::new("git")
            .args([
                "fetch",
                "origin",
                &format!("pull/{pr_number}/head"),
            ])
            .current_dir(repo_path)
            .status()?;
        if !status.success() {
            anyhow::bail!("Failed to fetch PR #{pr_number} (neither merge nor head ref)");
        }
    }

    // Checkout the fetched commit
    let checkout_status = Command::new("git")
        .args(["checkout", "--detach", "FETCH_HEAD"])
        .current_dir(repo_path)
        .status()?;

    if !checkout_status.success() {
        anyhow::bail!("Failed to checkout FETCH_HEAD for PR #{pr_number} in {}", repo_path.display());
    }

    Ok(())
}
