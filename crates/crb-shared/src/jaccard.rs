use std::collections::HashSet;

use crate::normalize_text;

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
    let tokenize = |s: &str| -> Vec<String> {
        let text = if normalize_markdown {
            normalize_text(s)
        } else {
            s.to_lowercase()
        };
        text.split_whitespace()
            .map(|w| w.to_string())
            .filter(|w| !w.is_empty())
            .collect()
    };

    let words_a: HashSet<_> = tokenize(a).into_iter().collect();
    let words_b: HashSet<_> = tokenize(b).into_iter().collect();

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
        assert_eq!(jaccard_similarity("hello", "world", false), 0.0);
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
    fn jaccard_normalize_collapses_whitespace() {
        assert_eq!(
            jaccard_similarity("hello    world", "hello world", true),
            1.0
        );
    }
}
