//! Shared types used by both `crb-webui-backend` (server) and `crb-webui-frontend` (WASM).

pub mod adhoc;
pub mod admin;
pub mod auth;
pub mod config;
pub mod routes;
pub mod runs;

/// Deterministic HSL color from a role abbreviation — no hardcoded color map.
/// Each role gets a unique hue via a simple hash of its abbreviation bytes.
pub fn role_color(role: &str) -> String {
    let hash: u32 = role
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let hue = hash % 360;
    format!("hsl({hue}, 65%, 55%)")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same input must always produce the same color.
    #[test]
    fn test_role_color_determinism() {
        let inputs = ["reviewer", "coder", "tester", "pm", "architect"];
        for input in inputs {
            let a = role_color(input);
            let b = role_color(input);
            insta::assert_debug_snapshot!((input, a, b));
        }
    }

    /// Different inputs should produce distinct colors.
    /// (Collisions are possible in theory since hue range is 360, but for
    /// a reasonable set of distinct abbreviations they should differ.)
    #[test]
    fn test_role_color_uniqueness() {
        let inputs = ["reviewer", "coder", "tester", "pm", "architect", "devops"];
        let mut colors: Vec<String> = inputs.iter().map(|s| role_color(s)).collect();
        colors.sort();
        colors.dedup();
        insta::assert_debug_snapshot!(colors.len());
        insta::assert_debug_snapshot!(inputs.len());
    }

    /// Empty input must produce a defined color string at a valid hue.
    #[test]
    fn test_role_color_empty_input() {
        let color = role_color("");
        assert!(
            color.starts_with("hsl("),
            "empty input should produce a valid hsl: got {color}"
        );
        assert!(
            color.ends_with(", 65%, 55%)"),
            "empty input should have saturation=65%% lightness=55%%: got {color}"
        );
    }

    /// Long role names should not panic and should produce valid output.
    #[test]
    fn test_role_color_long_input() {
        let long = "a".repeat(10_000);
        let color = role_color(&long);
        assert!(
            color.starts_with("hsl("),
            "long input should produce a valid hsl: got {color}"
        );
    }

    /// Output must match the pattern "hsl(NNN, 65%, 55%)" and hue in [0, 359].
    #[test]
    fn test_role_color_format() {
        let inputs = ["", "a", "reviewer", "data-scientist"];
        for input in inputs {
            let color = role_color(input);
            let prefix = color.strip_suffix(", 65%, 55%)");
            assert!(
                prefix.is_some(),
                "role_color({input:?}) should end with ', 65%, 55%)': got {color}"
            );
            let prefix = prefix.unwrap();
            let hue_str = prefix.strip_prefix("hsl(");
            assert!(
                hue_str.is_some(),
                "role_color({input:?}) should start with 'hsl(': got {color}"
            );
            let hue: u32 = hue_str.unwrap().parse().unwrap_or(u32::MAX);
            assert!(
                hue < 360,
                "role_color({input:?}) hue should be in [0, 359]: got {hue}"
            );
        }
    }

    /// Hue should be in range 0..360 by construction (hash % 360).
    #[test]
    fn test_role_color_hue_range() {
        for n in 0..1000 {
            let input = format!("role-{n}");
            let color = role_color(&input);
            let hue_str = color
                .strip_prefix("hsl(")
                .and_then(|s| s.split_once(','))
                .map(|(h, _)| h);
            let hue: u32 = hue_str.unwrap().parse().expect("hue should be parseable");
            assert!(hue < 360, "hue {hue} out of range for input {input}");
        }
    }
}
