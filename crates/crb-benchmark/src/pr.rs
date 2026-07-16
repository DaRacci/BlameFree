use crb_shared::url::HasUrl;

/// Filter a list of PR entries by a comma-separated filter string.
///
/// Supports several match modes:
/// * `owner/repo/N` — exact PR number within a repository.
/// * `N` — bare number, matched exactly against the PR number portion of the
///   URL.
/// * `owner/repo/pull/1` — exact URL suffix.
/// * Any substring that appears in the lowercased URL.
///
/// When no PRs match, a warning is logged with available URLs.
pub fn filter_prs_by_pattern<T>(all_prs: Vec<T>, filter: &str) -> Vec<T>
where
    T: HasUrl + Clone,
{
    let filter_patterns: std::collections::HashSet<String> =
        filter.split(',').map(|s| s.trim().to_lowercase()).collect();

    let available_urls: Vec<String> = all_prs.iter().map(|pr| pr.url().to_string()).collect();

    let filtered: Vec<_> = all_prs
        .into_iter()
        .filter(|pr| {
            let url_lower = pr.url().to_lowercase();
            filter_patterns.iter().any(|pattern| {
                // Parse pattern as "repo/N" where N is a PR number
                if let Some((repo_part, pr_num_str)) = pattern.split_once('/') {
                    if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                        // Exact PR number match: `/pull/N` must NOT be followed by a digit
                        let pr_tag = format!("/pull/{pr_num}");
                        if let Some(pos) = url_lower.find(&pr_tag) {
                            let after = &url_lower[pos + pr_tag.len()..];
                            if after.is_empty() || !after.chars().next().unwrap().is_ascii_digit() {
                                if url_lower.contains(repo_part) {
                                    return true;
                                }
                            }
                        }
                    }
                }
                // Exact match only — avoid substring bugs where "1" matches "/pull/10".
                if let Ok(num) = pattern.parse::<u32>() {
                    // Bare number: match exactly against the PR number from the URL.
                    url_lower
                        .rsplit('/')
                        .next()
                        .and_then(|s| s.parse::<u32>().ok())
                        == Some(num)
                } else {
                    // Non-numeric fallback: exact URL suffix match (e.g. "repo/pull/1").
                    url_lower.ends_with(&format!("/{pattern}"))
                }
            })
        })
        .collect();

    if filtered.is_empty() {
        tracing::warn!(
            "PR filter \"{filter}\" matched no PRs. Available URLs:\n  {}",
            available_urls.join("\n  ")
        );
    }

    filtered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestPr(String);

    impl HasUrl for TestPr {
        fn url(&self) -> &str {
            &self.0
        }
    }

    #[test]
    fn test_filter_by_exact_pr_number() {
        let prs = vec![
            TestPr("https://github.com/owner/repo/pull/1".into()),
            TestPr("https://github.com/owner/repo/pull/2".into()),
            TestPr("https://github.com/owner/repo/pull/10".into()),
        ];
        let result = filter_prs_by_pattern(prs, "1");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_by_repo_and_number() {
        let prs = vec![
            TestPr("https://github.com/owner/repo/pull/42".into()),
            TestPr("https://github.com/other/repo/pull/42".into()),
        ];
        let result = filter_prs_by_pattern(prs, "owner/repo/pull/42");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_by_url_suffix() {
        let prs = vec![TestPr("https://github.com/owner/repo/pull/1".into())];
        let result = filter_prs_by_pattern(prs, "owner/repo/pull/1");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_comma_separated() {
        let prs = vec![
            TestPr("https://github.com/owner/repo/pull/1".into()),
            TestPr("https://github.com/owner/repo/pull/2".into()),
            TestPr("https://github.com/owner/repo/pull/3".into()),
        ];
        let result = filter_prs_by_pattern(prs, "1,3");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_no_match_returns_empty() {
        let prs = vec![
            TestPr("https://github.com/owner/repo/pull/5".into()),
            TestPr("https://github.com/owner/repo/pull/6".into()),
        ];
        let result = filter_prs_by_pattern(prs, "999");
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_bare_number_no_substring_match() {
        let prs = vec![
            TestPr("https://github.com/owner/repo/pull/1".into()),
            TestPr("https://github.com/owner/repo/pull/10".into()),
            TestPr("https://github.com/owner/repo/pull/100".into()),
        ];
        let result = filter_prs_by_pattern(prs, "1");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_case_insensitive() {
        let prs = vec![TestPr("https://github.com/OWNER/REPO/pull/1".into())];
        let result = filter_prs_by_pattern(prs, "owner/repo/pull/1");
        assert_eq!(result.len(), 1);
    }
}
