//! URL parsing utilities.

use anyhow::{Result, anyhow};
use regex::Regex;

/// Parse a standard GitHub PR URL into `(owner, repo, pr_number)`.
///
/// Accepts URLs of the form `https://github.com/{owner}/{repo}/pull/{number}`.
/// Returns an error if the URL doesn't match the expected format.
pub fn parse_github_url(url: &str) -> Result<(String, String, u32)> {
    let re = Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/pull/(\d+)$")?;
    let caps = re
        .captures(url)
        .ok_or_else(|| anyhow!("Invalid GitHub PR URL: {url}"))?;
    let owner = caps[1].to_string();
    let repo = caps[2].to_string();
    let pr_number: u32 = caps[3].parse()?;
    Ok((owner, repo, pr_number))
}

/// A type that exposes a PR/issue URL string.
pub trait HasUrl {
    fn url(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_url() {
        let result = parse_github_url("https://github.com/owner/repo/pull/42");
        assert!(result.is_ok());
        let (owner, repo, num) = result.unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
        assert_eq!(num, 42);
    }

    #[test]
    fn valid_url_with_hyphens() {
        let result = parse_github_url("https://github.com/my-org/my-repo/pull/123");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            ("my-org".to_string(), "my-repo".to_string(), 123)
        );
    }

    #[test]
    fn not_github_url() {
        let result = parse_github_url("https://gitlab.com/owner/repo/merge_requests/1");
        assert!(result.is_err());
    }

    #[test]
    fn no_pr_number() {
        let result = parse_github_url("https://github.com/owner/repo");
        assert!(result.is_err());
    }

    #[test]
    fn empty_string() {
        let result = parse_github_url("");
        assert!(result.is_err());
    }

    #[test]
    fn non_numeric_pr() {
        let result = parse_github_url("https://github.com/owner/repo/pull/abc");
        assert!(result.is_err());
    }

    #[test]
    fn trailing_slash_rejected() {
        let result = parse_github_url("https://github.com/owner/repo/pull/42/");
        assert!(result.is_err());
    }

    #[test]
    fn non_https_rejected() {
        let result = parse_github_url("http://github.com/owner/repo/pull/42");
        assert!(result.is_err());
    }
}
