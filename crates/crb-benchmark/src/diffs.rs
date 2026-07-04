use std::path::Path;
use std::process::Command;

use anyhow::Result;
use tracing::info;

/// Extract diffs from base repos into persistent per-PR worktrees.
///
/// For each PR, creates a worktree at `{benchmark_dir}/worktrees/{owner}_{repo}_{pr}/`
/// and extracts a diff into `{benchmark_dir}/diffs/{owner}_{repo}_{pr}.diff`.
///
/// Skips if both worktree AND diff file already exist. Worktrees are PERSISTENT.
pub fn run(benchmark_dir: &Path) -> Result<()> {
    let base_repos_dir = benchmark_dir.join("base-repos");
    if !base_repos_dir.exists() {
        anyhow::bail!(
            "Base repos directory does not exist: {}. Run `crb-benchmark scaffold` first.",
            base_repos_dir.display()
        );
    }

    let diffs_dir = benchmark_dir.join("diffs");
    let worktrees_dir = benchmark_dir.join("worktrees");
    std::fs::create_dir_all(&diffs_dir)?;
    std::fs::create_dir_all(&worktrees_dir)?;

    info!("Scanning base repos in: {}", base_repos_dir.display());

    let mut diff_count = 0;
    let skipped_count = 0;

    for entry in std::fs::read_dir(&base_repos_dir)? {
        let entry = entry?;
        let repo_path = entry.path();
        if !repo_path.is_dir() || !repo_path.join(".git").exists() {
            continue;
        }

        let repo_name = repo_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Parse owner_repo into (owner, repo)
        // The repo_name format is "owner_repo" from scaffold step
        // We need to discover which PRs belong to this repo by checking
        // available remote refs

        // List all PR merge refs that were pre-fetched in scaffold
        let refs_output = Command::new("git")
            .args([
                "for-each-ref",
                "--format=%(refname:strip=4)",
                "refs/remotes/origin/pull/*/merge",
            ])
            .current_dir(&repo_path)
            .output()?;

        if !refs_output.status.success() {
            tracing::warn!("Failed to list PR refs for {}", repo_name);
            continue;
        }

        let refs_str = String::from_utf8_lossy(&refs_output.stdout);
        let pr_numbers: Vec<u32> = refs_str
            .lines()
            .filter_map(|line| {
                // Format: "N/merge" -> extract N
                line.split('/').next().and_then(|n| n.parse().ok())
            })
            .collect();

        if pr_numbers.is_empty() {
            // Try to find PR head refs as fallback
            let head_refs = Command::new("git")
                .args([
                    "for-each-ref",
                    "--format=%(refname:strip=4)",
                    "refs/remotes/origin/pull/*/head",
                ])
                .current_dir(&repo_path)
                .output()?;

            if head_refs.status.success() {
                let head_str = String::from_utf8_lossy(&head_refs.stdout);
                let head_nums: Vec<u32> = head_str
                    .lines()
                    .filter_map(|line| line.split('/').next().and_then(|n| n.parse().ok()))
                    .collect();
                if head_nums.is_empty() {
                    tracing::warn!("No PR refs found for {}", repo_name);
                    continue;
                }
                for pr_number in &head_nums {
                    diff_count += extract_diff_for_pr(
                        &repo_path,
                        &repo_name,
                        *pr_number,
                        &diffs_dir,
                        &worktrees_dir,
                        false,
                    )?;
                }
            } else {
                tracing::warn!("No PR refs found for {}", repo_name);
                continue;
            }
        } else {
            for pr_number in &pr_numbers {
                diff_count += extract_diff_for_pr(
                    &repo_path,
                    &repo_name,
                    *pr_number,
                    &diffs_dir,
                    &worktrees_dir,
                    true,
                )?;
            }
        }
    }

    let total = diff_count + skipped_count;
    info!(
        "Fetch-diffs complete: {} diff(s) extracted, {} skipped (already exist) out of {} total PRs",
        diff_count, skipped_count, total
    );

    if diff_count == 0 && skipped_count == 0 {
        println!("No diffs extracted. Run `crb-benchmark scaffold` first.");
    } else if skipped_count > 0 {
        println!("{} worktrees/diffs already exist (skipped)", skipped_count);
    }

    Ok(())
}

/// Extract diff for a single PR using persistent worktrees.
///
/// Skips if BOTH the worktree AND the diff file already exist.
/// Returns 1 if a new diff was extracted, 0 if skipped.
fn extract_diff_for_pr(
    repo_path: &Path,
    repo_name: &str,
    pr_number: u32,
    diffs_dir: &Path,
    worktrees_dir: &Path,
    use_merge_ref: bool,
) -> Result<u32> {
    let worktree_path = worktrees_dir.join(format!("{repo_name}_{pr_number}"));
    let diff_path = diffs_dir.join(format!("{repo_name}_{pr_number}.diff"));

    // Skip if BOTH worktree AND diff already exist
    if worktree_path.join(".git").exists() && diff_path.exists() {
        info!(
            "Worktree and diff already exist for PR #{} in {} (skipping)",
            pr_number, repo_name
        );
        return Ok(0);
    }

    let ref_name = if use_merge_ref {
        format!("refs/remotes/origin/pull/{pr_number}/merge")
    } else {
        format!("refs/remotes/origin/pull/{pr_number}/head")
    };

    // Create worktree if it doesn't exist
    if !worktree_path.join(".git").exists() {
        info!(
            "Creating worktree at {} for PR #{} in {}",
            worktree_path.display(),
            pr_number,
            repo_name
        );

        // Canonicalize worktree path to absolute - git worktree add resolves
        // relative paths relative to the repo, not CWD.
        let abs_worktree = if worktree_path.is_absolute() {
            worktree_path.clone()
        } else {
            std::env::current_dir()?.join(&worktree_path)
        };
        std::fs::create_dir_all(abs_worktree.parent().unwrap_or(&abs_worktree))?;

        let status = Command::new("git")
            .args([
                "worktree",
                "add",
                &abs_worktree.to_string_lossy(),
                &ref_name,
            ])
            .current_dir(repo_path)
            .status()?;

        if !status.success() {
            tracing::warn!(
                "Failed to create worktree for PR #{} in {}",
                pr_number,
                repo_name
            );
            return Ok(0);
        }
    }

    // Extract diff from worktree (HEAD^..HEAD for merge commits)
    let output = Command::new("git")
        .args(["diff", "HEAD^", "HEAD"])
        .current_dir(&worktree_path)
        .output()?;

    if output.status.success() {
        std::fs::write(&diff_path, &output.stdout)?;
        let line_count = String::from_utf8_lossy(&output.stdout).lines().count();
        info!(
            "Extracted diff for {} PR #{} ({} lines) -> {}",
            repo_name,
            pr_number,
            line_count,
            diff_path.display()
        );
        Ok(1)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // HEAD^ may fail if not a merge commit - try diff with no parent (--root)
        if stderr.contains("ambiguous argument") || stderr.contains("unknown revision") {
            tracing::warn!(
                "HEAD^ not available for {} PR #{}, trying --root diff",
                repo_name,
                pr_number
            );
            let root_output = Command::new("git")
                .args(["diff", "--root", "HEAD"])
                .current_dir(&worktree_path)
                .output()?;
            if root_output.status.success() {
                std::fs::write(&diff_path, &root_output.stdout)?;
                let line_count = String::from_utf8_lossy(&root_output.stdout).lines().count();
                info!(
                    "Extracted root diff for {} PR #{} ({} lines) -> {}",
                    repo_name,
                    pr_number,
                    line_count,
                    diff_path.display()
                );
                return Ok(1);
            } else {
                tracing::warn!(
                    "Failed to extract root diff for {} PR #{}: {}",
                    repo_name,
                    pr_number,
                    String::from_utf8_lossy(&root_output.stderr)
                );
            }
        } else {
            tracing::warn!(
                "Failed to extract diff for {} PR #{}: {}",
                repo_name,
                pr_number,
                stderr.trim()
            );
        }
        Ok(0)
    }
}
