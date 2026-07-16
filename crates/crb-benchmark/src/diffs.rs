use std::path::Path;
use std::process::Command;
use std::{env, fs};

use anyhow::{Result, bail};
use tracing::{info, warn};

/// Extract diffs from base repos into persistent per-PR worktrees.
///
/// For each PR, creates a worktree at `{benchmark_dir}/worktrees/{owner}_{repo}_{pr}/`
/// and extracts a diff into `{benchmark_dir}/diffs/{owner}_{repo}_{pr}.diff`.
///
/// Skips if both worktree AND diff file already exist. Worktrees are PERSISTENT.
pub fn run(benchmark_dir: &Path) -> Result<()> {
    let base_repos_dir = benchmark_dir.join("base-repos");
    if !base_repos_dir.exists() {
        bail!(
            "Base repos directory does not exist: {}. Run `crb-benchmark scaffold` first.",
            base_repos_dir.display()
        );
    }

    let diffs_dir = benchmark_dir.join("diffs");
    let worktrees_dir = benchmark_dir.join("worktrees");
    fs::create_dir_all(&diffs_dir)?;
    fs::create_dir_all(&worktrees_dir)?;

    info!("Scanning base repos in: {}", base_repos_dir.display());

    let mut diff_count = 0;
    let skipped_count = 0;

    for entry in fs::read_dir(&base_repos_dir)? {
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
            warn!("Failed to list PR refs for {}", repo_name);
            continue;
        }

        let refs_str = String::from_utf8_lossy(&refs_output.stdout);
        let pr_numbers: Vec<u32> = refs_str
            .lines()
            .filter_map(|line| line.split('/').next().and_then(|n| n.parse().ok()))
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
                    warn!("No PR refs found for {}", repo_name);
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
                    )? as u32;
                }
            } else {
                warn!("No PR refs found for {}", repo_name);
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
                )? as u32;
            }
        }
    }

    let total = diff_count + skipped_count;
    info!(
        "Fetch-diffs complete: {} diff(s) extracted, {} skipped (already exist) out of {} total PRs",
        diff_count, skipped_count, total
    );

    if diff_count == 0 && skipped_count == 0 {
        info!("No diffs extracted. Run `crb-benchmark scaffold` first.");
    } else if skipped_count > 0 {
        info!("{} worktrees/diffs already exist (skipped)", skipped_count);
    }

    Ok(())
}

/// Extract diff for a single PR using persistent worktrees.
///
/// Returns `bool` indicating whether a new diff was extracted.
fn extract_diff_for_pr(
    repo_path: &Path,
    repo_name: &str,
    pr_number: u32,
    diffs_dir: &Path,
    worktrees_dir: &Path,
    use_merge_ref: bool,
) -> Result<bool> {
    let worktree_path = worktrees_dir.join(format!("{repo_name}_{pr_number}"));
    let diff_path = diffs_dir.join(format!("{repo_name}_{pr_number}.diff"));

    if worktree_path.join(".git").exists() && diff_path.exists() {
        info!(
            "Worktree and diff already exist for PR #{} in {} (skipping)",
            pr_number, repo_name
        );
        return Ok(false);
    }

    let ref_name = if use_merge_ref {
        format!("refs/remotes/origin/pull/{pr_number}/merge")
    } else {
        format!("refs/remotes/origin/pull/{pr_number}/head")
    };

    if !worktree_path.join(".git").exists() {
        info!(
            "Creating worktree at {} for PR #{} in {}",
            worktree_path.display(),
            pr_number,
            repo_name
        );

        // Canonicalize worktree path to absolute
        // git worktree add resolves relative paths relative to the repo, not CWD.
        let abs_worktree = if worktree_path.is_absolute() {
            worktree_path.clone()
        } else {
            env::current_dir()?.join(&worktree_path)
        };
        fs::create_dir_all(abs_worktree.parent().unwrap_or(&abs_worktree))?;

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
            warn!(
                "Failed to create worktree for PR #{} in {}",
                pr_number, repo_name
            );
            return Ok(false);
        }
    }

    let output = Command::new("git")
        .args(["diff", "HEAD^", "HEAD"])
        .current_dir(&worktree_path)
        .output()?;

    if output.status.success() {
        fs::write(&diff_path, &output.stdout)?;
        let line_count = String::from_utf8_lossy(&output.stdout).lines().count();
        info!(
            "Extracted diff for {} PR #{} ({} lines) -> {}",
            repo_name,
            pr_number,
            line_count,
            diff_path.display()
        );
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // HEAD^ may fail if not a merge commit, try diff with no parent (--root)
        if stderr.contains("ambiguous argument") || stderr.contains("unknown revision") {
            warn!(
                "HEAD^ not available for {} PR #{}, trying --root diff",
                repo_name, pr_number
            );
            let root_output = Command::new("git")
                .args(["diff", "--root", "HEAD"])
                .current_dir(&worktree_path)
                .output()?;
            if root_output.status.success() {
                fs::write(&diff_path, &root_output.stdout)?;
                let line_count = String::from_utf8_lossy(&root_output.stdout).lines().count();
                info!(
                    "Extracted root diff for {} PR #{} ({} lines) -> {}",
                    repo_name,
                    pr_number,
                    line_count,
                    diff_path.display()
                );
                return Ok(true);
            } else {
                warn!(
                    "Failed to extract root diff for {} PR #{}: {}",
                    repo_name,
                    pr_number,
                    String::from_utf8_lossy(&root_output.stderr)
                );
            }
        } else {
            warn!(
                "Failed to extract diff for {} PR #{}: {}",
                repo_name,
                pr_number,
                stderr.trim()
            );
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crb_harness::test_utils::{setup_empty_commit_repo, setup_repo_with_diffs};

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

        fs::write(
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

        fs::write(repo_path.join("main.rs"), "fn main() {\n    // staged\n}\n").expect("write");
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
        let (_dir, repo_path) = setup_empty_commit_repo();

        let output = Command::new("git")
            .arg("diff")
            .current_dir(&repo_path)
            .output()
            .expect("git diff on empty");
        assert!(output.status.success());
        let diff = String::from_utf8_lossy(&output.stdout);
        assert!(diff.is_empty(), "empty repo should have no diff");
    }
}
