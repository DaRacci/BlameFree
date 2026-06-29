use std::path::Path;
use std::process::Command;

use anyhow::Result;
use regex::Regex;
use tracing::info;

/// Clone/fetch all repos referenced in the dataset, checking out PR merge commits.
pub fn run(dataset_dir: &Path, repos_dir: &Path) -> Result<()> {
    let entries = crb_reporting::load_golden_datasets(dataset_dir)?;
    std::fs::create_dir_all(repos_dir)?;

    let mut unique_repos = std::collections::BTreeSet::new();

    for entry in &entries {
        let (owner, repo_name, pr_number) = match parse_github_url(&entry.url) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Skipping entry with invalid URL '{}': {e}", entry.url);
                continue;
            }
        };

        let full_name = format!("{owner}/{repo_name}");
        let repo_path = repos_dir.join(format!("{owner}_{repo_name}"));

        if !repo_path.join(".git").exists() {
            info!("Cloning {}/{} into {}...", owner, repo_name, repo_path.display());
            let status = Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    &format!("https://github.com/{}/{}.git", owner, repo_name),
                    &repo_path.to_string_lossy(),
                ])
                .status()?;

            if !status.success() {
                tracing::warn!("Failed to clone {}/{}, skipping", owner, repo_name);
                continue;
            }
        } else {
            info!("Repo {full_name} already exists at {}", repo_path.display());
        }

        // Try to fetch and checkout the PR merge ref (pull/N/merge)
        let fetch_result = Command::new("git")
            .args(["fetch", "origin", &format!("pull/{pr_number}/merge")])
            .current_dir(&repo_path)
            .status();

        if let Ok(status) = fetch_result {
            if status.success() {
                let _ = Command::new("git")
                    .args(["checkout", "FETCH_HEAD"])
                    .current_dir(&repo_path)
                    .status();
                info!(
                    "Checked out PR #{} merge commit in {}",
                    pr_number,
                    repo_path.display()
                );
            } else {
                // Fallback: fetch the PR head ref
                tracing::warn!(
                    "PR #{} merge ref not available, falling back to PR head",
                    pr_number
                );
                let _ = Command::new("git")
                    .args(["fetch", "origin", &format!("pull/{pr_number}/head")])
                    .current_dir(&repo_path)
                    .status();
                let _ = Command::new("git")
                    .args(["checkout", "FETCH_HEAD"])
                    .current_dir(&repo_path)
                    .status();
                info!(
                    "Checked out PR #{} head commit in {}",
                    pr_number,
                    repo_path.display()
                );
            }
        } else {
            tracing::warn!("PR #{} fetch failed for {}, skipping", pr_number, full_name);
        }

        unique_repos.insert(full_name.clone());
    }

    info!(
        "Scaffold summary: {} unique repos processed for {} PRs",
        unique_repos.len(),
        entries.len()
    );

    if !unique_repos.is_empty() {
        println!("Scaffolded repos:");
        for repo in &unique_repos {
            println!("  https://github.com/{}.git", repo);
        }
    }

    Ok(())
}

/// Parse a GitHub PR URL into (owner, repo_name, pr_number).
///
/// Expects URLs of the form `https://github.com/owner/repo/pull/N`.
pub fn parse_github_url(url: &str) -> Result<(String, String, u32)> {
    let re = Regex::new(r"github\.com/([^/]+)/([^/]+)/pull/(\d+)")?;
    let caps = re
        .captures(url)
        .ok_or_else(|| anyhow::anyhow!("Invalid GitHub URL: {url}"))?;
    Ok((
        caps[1].to_string(),
        caps[2].to_string(),
        caps[3].parse()?,
    ))
}
