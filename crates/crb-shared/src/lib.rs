/// Default model for ad-hoc and judge review tasks.
pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-flash";

/// Default model for benchmark/harness reviews (often a larger model).
pub const DEFAULT_MODEL_PRO: &str = "deepseek/deepseek-v4-pro";

pub mod cache;
pub mod deduplicate;
pub mod finding;
pub mod jaccard;
pub mod metrics;
pub mod pattern;
pub mod severity;

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

/// Strip markdown formatting characters and normalize whitespace.
///
/// Lowercases, removes common markdown sigils (`*`, `_`, `` ` ``, `#`, `[`,
/// `]`), and collapses multiple whitespace into single spaces.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_via_utils() {
        assert_eq!(sanitize_filename("hello world"), "hello_world");
        assert_eq!(sanitize_filename("file.name.txt"), "file_name_txt");
        assert_eq!(sanitize_filename("already_ok"), "already_ok");
        assert_eq!(sanitize_filename(""), "");
        assert_eq!(sanitize_filename("a|b<c>d:e"), "a_b_c_d_e");
    }

    #[test]
    fn normalize_strips_markdown() {
        let n = normalize_text(" **CRITICAL**: This is a *test* ");
        assert!(!n.contains('*'));
        assert!(!n.contains('#'));
        assert_eq!(n, "critical: this is a test");
    }
}
