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

    /// Known SHA256 hex digest of the empty string.
    const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    /// Known SHA256 hex digest of "hello".
    const HELLO_HASH: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    /// Known SHA256 hex digest of "world".
    const WORLD_HASH: &str = "486ea46224d1bb4fb680f34f7c9ad96a8f24ec88be73ea8e5a6c65260e9cb8a7";

    #[test]
    fn test_sha256_hex_empty_string() {
        let h = sha256_hex("");
        assert_eq!(h, EMPTY_HASH);
    }

    #[test]
    fn test_sha256_hex_known_value_hello() {
        let h = sha256_hex("hello");
        assert_eq!(h, HELLO_HASH);
    }

    #[test]
    fn test_sha256_hex_known_value_world() {
        let h = sha256_hex("world");
        assert_eq!(h, WORLD_HASH);
    }

    #[test]
    fn test_sha256_hex_length() {
        let h = sha256_hex("hello");
        assert_eq!(h.len(), 64); // SHA256 hex is 64 chars
    }

    #[test]
    fn test_sha256_hex_deterministic() {
        let a = sha256_hex("hello");
        let b = sha256_hex("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn test_sha256_hex_different_inputs() {
        let a = sha256_hex("hello");
        let b = sha256_hex("world");
        assert_ne!(a, b);
    }
}
