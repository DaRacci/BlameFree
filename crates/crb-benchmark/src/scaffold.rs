use std::collections::{BTreeMap, BTreeSet};
use std::fs::create_dir_all;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use crb_reporting::golden::load_golden_datasets;
use regex::Regex;
use tracing::{info, warn};

/// Clone/fetch all repos referenced in the dataset into a unified benchmark directory.
///
/// Layout:
///   {benchmark_dir}/base-repos/{owner}_{repo}/    - shallow clones, never checked out directly
///   {benchmark_dir}/diffs/                          - pre-extracted per-PR diffs
///   {benchmark_dir}/worktrees/{owner}_{repo}_{pr}/  - persistent per-PR worktrees
pub fn run(dataset_dir: &Path, benchmark_dir: &Path) -> Result<()> {
    let entries = load_golden_datasets(dataset_dir)?;

    let base_repos_dir = benchmark_dir.join("base-repos");
    create_dir_all(&base_repos_dir)?;

    create_dir_all(benchmark_dir.join("diffs"))?;
    create_dir_all(benchmark_dir.join("worktrees"))?;

    let mut repo_prs: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();

    for entry in &entries {
        let (owner, repo_name, pr_number) = match parse_github_url(&entry.url) {
            Ok(v) => v,
            Err(e) => {
                warn!("Skipping entry with invalid URL '{}': {e}", entry.url);
                continue;
            }
        };
        repo_prs
            .entry((owner, repo_name))
            .or_default()
            .push(pr_number);
    }

    let mut unique_repos = BTreeSet::new();
    let mut total_fetched = 0usize;

    for ((owner, repo_name), pr_numbers) in &repo_prs {
        let full_name = format!("{owner}/{repo_name}");
        let repo_path = base_repos_dir.join(format!("{owner}_{repo_name}"));

        if !repo_path.join(".git").exists() {
            info!(
                "Cloning {}/{} into {}...",
                owner,
                repo_name,
                repo_path.display()
            );
            let status = Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "--single-branch",
                    &format!("https://github.com/{}/{}.git", owner, repo_name),
                    &repo_path.to_string_lossy(),
                ])
                .status()?;

            if !status.success() {
                warn!("Failed to clone {}/{}, skipping", owner, repo_name);
                continue;
            }
        } else {
            info!("Repo {full_name} already exists at {}", repo_path.display());
        }

        // Pre-fetch ALL PR merge refs for this repo (not just the current PR)
        for pr_num in pr_numbers {
            let fetch_result = Command::new("git")
                .args([
                    "fetch",
                    "origin",
                    &format!("pull/{pr_num}/merge:refs/remotes/origin/pull/{pr_num}/merge"),
                ])
                .current_dir(&repo_path)
                .status();

            match fetch_result {
                Ok(status) if status.success() => {
                    info!(
                        "Fetched PR #{} merge ref for {}/{}",
                        pr_num, owner, repo_name
                    );
                    total_fetched += 1;
                }
                Ok(_) => {
                    // Fallback: fetch PR head ref
                    warn!(
                        "PR #{} merge ref not available for {}/{}, trying PR head",
                        pr_num, owner, repo_name
                    );
                    let head_status = Command::new("git")
                        .args([
                            "fetch",
                            "origin",
                            &format!("pull/{pr_num}/head:refs/remotes/origin/pull/{pr_num}/head"),
                        ])
                        .current_dir(&repo_path)
                        .status();

                    match head_status {
                        Ok(status) if status.success() => {
                            info!(
                                "Fetched PR #{} head ref for {}/{}",
                                pr_num, owner, repo_name
                            );
                            total_fetched += 1;
                        }
                        Ok(_) => {
                            // Last resort: try GitHub API via `gh pr view` to get merge commit SHA
                            warn!(
                                "PR #{} head ref also unavailable for {}/{}, trying GitHub API for commit SHA",
                                pr_num, owner, repo_name
                            );
                            let pr_url =
                                format!("https://github.com/{owner}/{repo_name}/pull/{pr_num}");
                            let gh_output = Command::new("gh")
                                .args([
                                    "pr",
                                    "view",
                                    &pr_url,
                                    "--json",
                                    "mergeCommit",
                                    "--jq",
                                    ".mergeCommit.oid",
                                ])
                                .output();
                            match gh_output {
                                Ok(output) if output.status.success() => {
                                    let sha =
                                        String::from_utf8_lossy(&output.stdout).trim().to_string();
                                    if !sha.is_empty() {
                                        info!(
                                            "Got merge commit SHA {} for PR #{} via GitHub API",
                                            &sha[..8.min(sha.len())],
                                            pr_num
                                        );
                                        let fetch_sha = Command::new("git")
                                            .args(["fetch", "origin", &sha])
                                            .current_dir(&repo_path)
                                            .status();
                                        if let Ok(s) = fetch_sha {
                                            if s.success() {
                                                info!(
                                                    "Fetched commit {} for PR #{}",
                                                    &sha[..8],
                                                    pr_num
                                                );
                                                total_fetched += 1;
                                            } else {
                                                warn!(
                                                    "Failed to fetch commit {} for PR #{}",
                                                    &sha[..8],
                                                    pr_num
                                                );
                                            }
                                        }
                                    } else {
                                        warn!(
                                            "No merge commit SHA for PR #{} (likely still open)",
                                            pr_num
                                        );
                                    }
                                }
                                _ => {
                                    warn!("GitHub API failed for PR #{} (gh not available or rate-limited)", pr_num);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Head ref fetch spawn failed for PR #{}: {}", pr_num, e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Fetch failed for PR #{} in {}: {}", pr_num, full_name, e);
                }
            }
        }

        unique_repos.insert(full_name.clone());
    }

    info!(
        "Scaffold summary: {} unique repos processed, {} PR merge refs pre-fetched",
        unique_repos.len(),
        total_fetched
    );

    if !unique_repos.is_empty() {
        println!("Scaffolded repos:");
        for repo in &unique_repos {
            println!("  https://github.com/{}.git", repo);
        }
        println!("Pre-fetched {} PR merge refs", total_fetched);
    }

    Ok(())
}

/// Parse a GitHub PR URL into (owner, repo_name, pr_number).
///
/// Expects URLs of the form `https://github.com/owner/repo/pull/N`.
pub(crate) fn parse_github_url(url: &str) -> Result<(String, String, u32)> {
    let re = Regex::new(r"github\.com/([^/]+)/([^/]+)/pull/(\d+)")?;
    let caps = re
        .captures(url)
        .ok_or_else(|| anyhow::anyhow!("Invalid GitHub URL: {url}"))?;
    Ok((caps[1].to_string(), caps[2].to_string(), caps[3].parse()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_url_valid() {
        let result = parse_github_url("https://github.com/owner/repo/pull/42");
        assert!(result.is_ok());
        let (owner, repo, num) = result.unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
        assert_eq!(num, 42);
    }

    #[test]
    fn parse_github_url_with_hyphens() {
        let result = parse_github_url("https://github.com/my-org/my-repo/pull/123");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            ("my-org".to_string(), "my-repo".to_string(), 123)
        );
    }

    #[test]
    fn parse_github_url_not_github() {
        let result = parse_github_url("https://gitlab.com/owner/repo/merge_requests/1");
        assert!(result.is_err());
    }

    #[test]
    fn parse_github_url_no_pr_number() {
        let result = parse_github_url("https://github.com/owner/repo");
        assert!(result.is_err());
    }

    #[test]
    fn parse_github_url_empty() {
        let result = parse_github_url("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_github_url_non_numeric_pr() {
        let result = parse_github_url("https://github.com/owner/repo/pull/abc");
        assert!(result.is_err());
    }

    #[test]
    fn parse_github_url_trailing_slash() {
        // The regex is not $ anchored, so trailing slash still matches
        let result = parse_github_url("https://github.com/owner/repo/pull/42/");
        assert!(result.is_ok());
        let (owner, repo, num) = result.unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
        assert_eq!(num, 42);
    }
}
