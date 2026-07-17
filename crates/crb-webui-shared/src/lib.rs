//! Shared types used by both `crb-webui-backend` (server) and `crb-webui-frontend` (WASM).

pub mod adhoc;
pub mod admin;
pub mod auth;
pub mod config;
pub mod routes;
pub mod runs;

/// Deterministically maps an agent abbreviation string to a hex colour code.
///
/// The same input always produces the same output.
/// Different agent abbreviations generally produce different colours.
///
/// Empty strings return a neutral gray.
pub fn role_color(role: &str) -> String {
    if role.is_empty() {
        return "#808080".to_string();
    }
    let hash = fnv1a_hash(role);
    let r = ((hash >> 16) & 0xFF) as u8;
    let g = ((hash >> 8) & 0xFF) as u8;
    let b = (hash & 0xFF) as u8;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

/// Simple FNV-1a hash for stable, deterministic string hashing.
fn fnv1a_hash(s: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_role_color_determinism() {
        let color1 = role_color("FE");
        let color2 = role_color("FE");
        insta::assert_debug_snapshot!((color1, color2));
    }

    #[test]
    fn test_role_color_uniqueness() {
        let roles = [
            "FE", "BE", "SEC", "INFRA", "DOCS", "UX", "QA", "DEVOPS", "ML",
        ];
        let mut colors = HashSet::new();
        for role in &roles {
            let c = role_color(role);
            assert!(
                colors.insert(c.clone()),
                "duplicate color for role {}: {}",
                role,
                c
            );
        }
    }

    #[test]
    fn test_role_color_empty() {
        insta::assert_debug_snapshot!(role_color(""));
    }

    #[test]
    fn test_role_color_consistency_across_calls() {
        let roles = ["FE", "BE", "SEC", "INFRA"];
        for role in &roles {
            let c1 = role_color(role);
            let c2 = role_color(role);
            insta::assert_debug_snapshot!((role, c1, c2));
        }
    }

    #[test]
    fn test_role_color_valid_hex() {
        let color = role_color("FE");
        assert!(color.starts_with('#'));
        assert_eq!(color.len(), 7);
        // Verify all chars after # are hex
        assert!(color[1..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_fnv1a_hash_determinism() {
        insta::assert_debug_snapshot!((
            fnv1a_hash("hello"),
            fnv1a_hash("hello"),
            fnv1a_hash("world")
        ));
    }
}
