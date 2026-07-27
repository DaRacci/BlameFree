use rand::RngExt;
use rand::distr::Alphanumeric;

/// Generate a random alphanumeric string of the given length.
pub fn random_string(length: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

/// Strip markdown formatting characters and normalize whitespace.
///
/// Lowercases, removes common markdown sigils (`*`, `_`, `` ` ``, `#`, `[`,`]`),
/// and collapses multiple whitespace into single spaces.
pub fn normalize_text(text: &str) -> String {
    let text = text.to_lowercase();
    let text: String = text
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '[' | ']'))
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    // Collapse multiple spaces into one
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Sanitize a string for use as a filename.
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_string_length() {
        let s = random_string(32);
        assert_eq!(s.len(), 32);
    }

    #[test]
    fn test_random_string_zero_length() {
        let s = random_string(0);
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn test_random_string_alphanumeric() {
        let s = random_string(100);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_random_string_uniqueness() {
        // With string length 8 there are 62^8 ≈ 2.18e14 possible strings.
        // Generating 10,000 strings gives a collision probability near zero
        // (birthday bound: n²/(2d) = 1e8 / 4.36e14 ≈ 2.3e-7).
        const NUM_TESTS: usize = 10_000;
        const STRING_LENGTH: usize = 8;

        let strings: Vec<String> = (0..NUM_TESTS)
            .map(|_| random_string(STRING_LENGTH))
            .collect();

        let unique_strings: std::collections::HashSet<_> = strings.iter().collect();
        assert_eq!(
            unique_strings.len(),
            NUM_TESTS,
            "Expected all {} strings to be unique, but got {} unique",
            NUM_TESTS,
            unique_strings.len()
        );
    }

    #[test]
    fn normalize_strips_markdown() {
        let n = normalize_text(" **CRITICAL**: This is a *test* ");
        assert!(!n.contains('*'));
        assert!(!n.contains('#'));
        assert_eq!(n, "critical: this is a test");
    }

    #[test]
    fn sanitize_filename_via_utils() {
        assert_eq!(sanitize_filename("hello world"), "hello_world");
        assert_eq!(sanitize_filename("file.name.txt"), "file_name_txt");
        assert_eq!(sanitize_filename("already_ok"), "already_ok");
        assert_eq!(sanitize_filename(""), "");
        assert_eq!(sanitize_filename("a|b<c>d:e"), "a_b_c_d_e");
    }
}
