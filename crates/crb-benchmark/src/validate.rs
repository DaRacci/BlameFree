use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::{Result, bail};
use crb_shared::severity::Severity;
use strum::IntoEnumIterator;
use tracing::{info, warn};

/// Validate a golden dataset for integrity.
///
/// Checks:
/// 1. Each PR has a valid URL
/// 2. No duplicate URLs or PR titles
/// 3. Each golden comment has non-empty `comment` and valid `severity`
/// 4. Report total PRs, golden comments, unique repos
pub(crate) fn run_validate(dataset_dir: &Path) -> Result<()> {
    let entries = crb_reporting::golden::load_golden_datasets(dataset_dir)?;

    if entries.is_empty() {
        bail!("No entries found in dataset: {}", dataset_dir.display());
    }

    let mut errors: Vec<String> = Vec::new();
    let mut seen_urls = HashSet::new();
    let mut seen_titles = HashSet::new();
    let mut repos = BTreeSet::new();
    let mut total_golden_comments = 0usize;
    let valid_severities = Severity::iter().map(|v| v.to_string()).collect::<Vec<_>>();

    for entry in &entries {
        if entry.url.is_empty() {
            errors.push(format!("PR '{}' has empty URL", entry.pr_title));
        } else if !entry.url.starts_with("http") {
            errors.push(format!(
                "PR '{}' has invalid URL (does not start with http): {}",
                entry.pr_title, entry.url
            ));
        }

        if !seen_urls.insert(entry.url.clone()) {
            errors.push(format!("Duplicate URL: {}", entry.url));
        }

        if !seen_titles.insert(entry.pr_title.clone()) {
            errors.push(format!("Duplicate PR title: {}", entry.pr_title));
        }

        let repo_name = entry
            .url
            .trim_end_matches('/')
            .rsplit('/')
            .nth(2)
            .unwrap_or("unknown");
        repos.insert(repo_name.to_string());

        for (i, comment) in entry.comments.iter().enumerate() {
            total_golden_comments += 1;

            if comment.comment.trim().is_empty() {
                errors.push(format!(
                    "PR '{}' comment #{} has empty comment text",
                    entry.pr_title, i
                ));
            }

            if !valid_severities.contains(&comment.severity.to_string()) {
                errors.push(format!(
                    "PR '{}' comment #{} has unknown severity '{}' (expected one of: {})",
                    entry.pr_title,
                    i,
                    comment.severity,
                    valid_severities.join(", ")
                ));
            }
        }
    }

    println!("Dataset validation report for: {}", dataset_dir.display());
    println!("  PRs:           {}", entries.len());
    println!("  Golden comments: {}", total_golden_comments);
    println!("  Unique repos:   {}", repos.len());
    println!(
        "  Repos: {}",
        repos.iter().cloned().collect::<Vec<_>>().join(", ")
    );

    if errors.is_empty() {
        println!("  Status: ✅ All checks passed");
        info!("Dataset validation passed for {}", dataset_dir.display());
        Ok(())
    } else {
        println!("  Status: ❌ {} error(s) found", errors.len());
        for err in &errors {
            warn!("Validation error: {}", err);
            println!("    - {}", err);
        }
        bail!("Dataset validation failed with {} error(s)", errors.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_validate_empty_directory() {
        if let Ok(dir) = TempDir::new() {
            let result = run_validate(dir.path());
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_validate_valid_dataset() {
        if let Ok(dir) = TempDir::new() {
            let dataset = r#"{"entries":[{"pr_title":"Test","url":"https://github.com/a/b/pull/1","comments":[{"comment":"bug","severity":"critical"}]}]}"#;
            if let Ok(()) = std::fs::write(dir.path().join("dataset.json"), dataset) {
                let result = run_validate(dir.path());
                assert!(result.is_ok());
            }
        }
    }

    #[test]
    fn test_validate_duplicate_urls() {
        if let Ok(dir) = TempDir::new() {
            let dataset = r#"{"entries":[{"pr_title":"First","url":"https://github.com/a/b/pull/1","comments":[{"comment":"bug","severity":"critical"}]},{"pr_title":"Second","url":"https://github.com/a/b/pull/1","comments":[{"comment":"security","severity":"critical"}]}]}"#;
            if let Ok(()) = std::fs::write(dir.path().join("dataset.json"), dataset) {
                let result = run_validate(dir.path());
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn test_validate_duplicate_pr_titles() {
        if let Ok(dir) = TempDir::new() {
            let dataset = r#"{"entries":[{"pr_title":"Same","url":"https://github.com/a/b/pull/1","comments":[{"comment":"bug","severity":"critical"}]},{"pr_title":"Same","url":"https://github.com/a/b/pull/2","comments":[{"comment":"security","severity":"critical"}]}]}"#;
            if let Ok(()) = std::fs::write(dir.path().join("dataset.json"), dataset) {
                let result = run_validate(dir.path());
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn test_validate_empty_comment() {
        if let Ok(dir) = TempDir::new() {
            let dataset = r#"{"entries":[{"pr_title":"Test","url":"https://github.com/a/b/pull/1","comments":[{"comment":"","severity":"critical"}]}]}"#;
            if let Ok(()) = std::fs::write(dir.path().join("dataset.json"), dataset) {
                let result = run_validate(dir.path());
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn test_validate_invalid_severity() {
        if let Ok(dir) = TempDir::new() {
            let dataset = r#"{"entries":[{"pr_title":"Test","url":"https://github.com/a/b/pull/1","comments":[{"comment":"bug","severity":"super-critical"}]}]}"#;
            if let Ok(()) = std::fs::write(dir.path().join("dataset.json"), dataset) {
                let result = run_validate(dir.path());
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn test_validate_empty_url() {
        if let Ok(dir) = TempDir::new() {
            let dataset = r#"{"entries":[{"pr_title":"Test","url":"","comments":[{"comment":"bug","severity":"critical"}]}]}"#;
            if let Ok(()) = std::fs::write(dir.path().join("dataset.json"), dataset) {
                let result = run_validate(dir.path());
                assert!(result.is_err());
            }
        }
    }
}
