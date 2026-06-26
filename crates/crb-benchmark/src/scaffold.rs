use std::path::Path;

use anyhow::Result;
use tracing::info;

/// Clone/fetch all repos referenced in the dataset.
pub fn run(dataset_dir: &Path, repos_dir: &Path) -> Result<()> {
    let entries = crb_reporting::load_golden_datasets(dataset_dir)?;

    if !repos_dir.exists() {
        std::fs::create_dir_all(repos_dir)?;
        info!("Created repos directory: {}", repos_dir.display());
    }

    let mut unique_repos = std::collections::BTreeSet::new();

    for entry in &entries {
        // Extract repo owner/name from URL: "https://github.com/owner/repo/pull/N"
        let parts: Vec<&str> = entry.url.trim_end_matches('/').rsplit('/').collect();
        if parts.len() >= 3 {
            let pr_number = parts[0];
            let repo_name = parts[2];
            let full_name = format!("{}/{}", parts[2], parts[1]); // owner/repo
            unique_repos.insert(full_name.clone());

            let repo_path = repos_dir.join(repo_name);
            if repo_path.exists() {
                info!("Repo {} already exists at {}", full_name, repo_path.display());
            } else {
                info!(
                    "[PLACEHOLDER] Would clone {} into {} (PR #{})",
                    full_name,
                    repo_path.display(),
                    pr_number
                );
            }
        }
    }

    info!(
        "Scaffold summary: {} unique repos needed for {} PRs",
        unique_repos.len(),
        entries.len()
    );

    if !unique_repos.is_empty() {
        println!("Repos to clone:");
        for repo in &unique_repos {
            println!("  https://github.com/{}.git", repo);
        }
    }

    Ok(())
}
