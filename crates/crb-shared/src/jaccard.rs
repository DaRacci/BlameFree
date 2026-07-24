use std::collections::HashSet;

use crate::string::normalize_text;

/// Tokenize text exactly like Python's `.lower().split()`.
/// (whitespace split only, no punctuation stripping)
fn tokenize(text: &str, normalize_markdown: bool) -> Vec<String> {
    let text = match normalize_markdown {
        true => normalize_text(text),
        false => text.to_lowercase(),
    };

    text.to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Compute Jaccard word-overlap similarity between two strings.
///
/// Splits each string into lowercase word tokens and computes `|intersection| / |union|`.
///
/// If `normalize_markdown` is `true`, calls [`normalize_text`] first to strip markdown formatting and punctuation.
/// Otherwise only lowercases and splits on whitespace.
///
/// Returns `0.0` if either string produces no tokens.
///
/// # Examples
///
/// ```rust
/// use crb_shared::jaccard::jaccard_similarity;
///
/// // Identical strings
/// assert_eq!(jaccard_similarity("hello world", "hello world", false), 1.0);
///
/// // Partial overlap
/// let score = jaccard_similarity("hello world", "hello there", false);
/// assert!(score > 0.0 && score < 1.0);
///
/// // With markdown normalization
/// let score = jaccard_similarity("**hello** world", "hello world", true);
/// assert_eq!(score, 1.0);
/// ```
pub fn jaccard_similarity(a: &str, b: &str, normalize_markdown: bool) -> f64 {
    let words_a: HashSet<_> = tokenize(a, normalize_markdown).into_iter().collect();
    let words_b: HashSet<_> = tokenize(b, normalize_markdown).into_iter().collect();

    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jaccard_identical() {
        assert_eq!(
            jaccard_similarity("hello world foo bar", "hello world foo bar", false),
            1.0
        );
    }

    #[test]
    fn jaccard_partial() {
        let score = jaccard_similarity("hello world foo bar", "hello world baz qux", false);
        assert!(score > 0.0 && score < 1.0);
    }

    #[test]
    fn jaccard_no_overlap() {
        assert_eq!(
            jaccard_similarity("null pointer check", "SQL injection vulnerability", false),
            0.0
        );
    }

    #[test]
    fn jaccard_empty() {
        assert_eq!(jaccard_similarity("", "hello", false), 0.0);
        assert_eq!(jaccard_similarity("hello", "", false), 0.0);
    }

    #[test]
    fn jaccard_case_insensitive() {
        assert_eq!(jaccard_similarity("HELLO WORLD", "hello world", false), 1.0);
    }

    #[test]
    fn jaccard_with_normalize() {
        let raw = jaccard_similarity("**hello** world", "hello world", false);
        assert!(raw < 1.0);

        let norm = jaccard_similarity("**hello** world", "hello world", true);
        assert_eq!(norm, 1.0);
    }

    #[test]
    fn test_jaccard_punctuation_stripping() {
        // With whitespace-only split the parenthesized variant yields tokens
        // {"xss", "(cross-site", "scripting)"} — union of 6 → Jaccard = 1/6 ≈ 0.167
        let s1 = jaccard_similarity(
            "xss (cross-site scripting)",
            "xss cross site scripting",
            false,
        );

        assert!(s1 > 0.12 && (s1 - 1.0 / 6.0).abs() < 0.01);
    }

    #[test]
    fn test_jaccard_precise_intersection() {
        // "hardcoded" shared out of 7 unique words across
        // "hardcoded API key found" ∩ "hardcoded secret in config" = 1/7 ≈ 0.1428
        let score = jaccard_similarity(
            "hardcoded API key found",
            "hardcoded secret in config",
            false,
        );
        assert!(score > 0.12 && (score - 1.0 / 7.0).abs() < 0.01);
    }

    #[test]
    fn jaccard_normalize_collapses_whitespace() {
        assert_eq!(
            jaccard_similarity("hello    world", "hello world", true),
            1.0
        );
    }

    mod edge_cases {
        use super::*;

        #[test]
        fn test_jaccard_hyphen_difference() {
            // "cross-site" is a single token, "cross site" is two, they should have different intersection sizes
            let hyphen_score = jaccard_similarity(
                "cross-site scripting vulnerability",
                "cross site scripting",
                false,
            );
            let regular_score = jaccard_similarity(
                "cross site scripting vulnerability",
                "cross site scripting",
                false,
            );

            // hyphen: 1 shared ("scripting") / 5 union = 0.2
            assert!(hyphen_score > 0.12 && (hyphen_score - 0.2).abs() < 0.01);

            // no hyphen: 3 shared / 4 union = 0.75
            assert!(regular_score > 0.12 && (regular_score - 0.75).abs() < 0.01);
        }

        #[test]
        fn test_jaccard_compound_difference() {
            // "well-known" is a single token, no overlap with "well" or "known"
            let score = jaccard_similarity("well-known vulnerability", "well known issue", false);
            assert!(
                score == 0.0,
                "well-known is a single token, no overlap with 'well' or 'known'"
            );
        }

        #[test]
        fn test_jaccard_apostrophe_preserved() {
            // "doesn't" is a single token (apostrophe preserved in whitespace split)
            let score = jaccard_similarity("doesn't work", "doesn't function", false);

            // {"doesn't"} common, union = {"doesn't", "work", "function"} = 3
            assert!(score > 0.12 && (score - 1.0 / 3.0).abs() < 0.01);
        }

        #[test]
        fn test_jaccard_ssrf_real_example() {
            // SSRF phrase tokens have zero overlap with expanded SSRF description
            let score = jaccard_similarity(
                "Server-Side Request Forgery via open()",
                "SSRF vulnerability using open(url) without validation",
                false,
            );
            assert!(score == 0.0);
        }
    }
}
