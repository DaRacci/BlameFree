/// Compute a SHA256 hex digest of the input string.
pub fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hex() {
        let h = sha256_hex("hello");
        assert_eq!(h.len(), 64); // SHA256 hex is 64 chars
    }
}
