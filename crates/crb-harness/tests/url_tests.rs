//! Tests for `extract_pr_info()`.
//!
//! Covers: valid GitHub URLs, trailing slashes, invalid URLs, empty strings.

// ---------------------------------------------------------------------------
// Valid GitHub URLs
// ---------------------------------------------------------------------------

#[test]
fn valid_github_url() {
    let result = crb_harness::extract_pr_info("https://github.com/owner/repo/pull/42");
    assert!(result.is_some());
    let (owner, repo, num) = result.unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(repo, "repo");
    assert_eq!(num, 42);
}

#[test]
fn valid_url_with_hyphens() {
    let result =
        crb_harness::extract_pr_info("https://github.com/discourse-graphite/discourse/pull/7");
    assert!(result.is_some());
    let (owner, repo, num) = result.unwrap();
    assert_eq!(owner, "discourse-graphite");
    assert_eq!(repo, "discourse");
    assert_eq!(num, 7);
}

#[test]
fn valid_url_large_pr_number() {
    let result = crb_harness::extract_pr_info("https://github.com/torvalds/linux/pull/1234567");
    assert!(result.is_some());
    assert_eq!(result.unwrap().2, 1_234_567);
}

#[test]
fn valid_url_single_char_names() {
    // Single-letter owner/repo should still match
    let result = crb_harness::extract_pr_info("https://github.com/a/b/pull/1");
    assert!(result.is_some());
    let (owner, repo, num) = result.unwrap();
    assert_eq!(owner, "a");
    assert_eq!(repo, "b");
    assert_eq!(num, 1);
}

// ---------------------------------------------------------------------------
// URL with trailing slash
// ---------------------------------------------------------------------------

#[test]
fn url_with_trailing_slash() {
    // The regex uses $ anchor, so trailing slash should NOT match
    let result = crb_harness::extract_pr_info("https://github.com/owner/repo/pull/42/");
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// Invalid URLs
// ---------------------------------------------------------------------------

#[test]
fn not_github_url() {
    let result = crb_harness::extract_pr_info("https://gitlab.com/owner/repo/merge_requests/1");
    assert!(result.is_none());
}

#[test]
fn no_pr_number() {
    let result = crb_harness::extract_pr_info("https://github.com/owner/repo");
    assert!(result.is_none());
}

#[test]
fn missing_pull_segment() {
    let result = crb_harness::extract_pr_info("https://github.com/owner/repo/issue/42");
    assert!(result.is_none());
}

#[test]
fn non_numeric_pr() {
    let result = crb_harness::extract_pr_info("https://github.com/owner/repo/pull/abc");
    assert!(result.is_none());
}

#[test]
fn http_instead_of_https() {
    // Regex expects https://
    let result = crb_harness::extract_pr_info("http://github.com/owner/repo/pull/42");
    assert!(result.is_none());
}

#[test]
fn www_prefix() {
    let result = crb_harness::extract_pr_info("https://www.github.com/owner/repo/pull/42");
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// Empty string
// ---------------------------------------------------------------------------

#[test]
fn empty_string() {
    let result = crb_harness::extract_pr_info("");
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn url_with_query_params() {
    let result = crb_harness::extract_pr_info("https://github.com/owner/repo/pull/42?diff=unified");
    assert!(result.is_none()); // regex ends with $, so query params break match
}

#[test]
fn url_with_fragment() {
    let result = crb_harness::extract_pr_info("https://github.com/owner/repo/pull/42#top");
    assert!(result.is_none());
}
