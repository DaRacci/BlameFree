use rand::{Rng, distributions::Alphanumeric};

/// Generate a random alphanumeric string of the given length.
pub fn random_string(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
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
}
