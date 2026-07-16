/// Aggregate summary for a run.
pub const SUMMARY_FILE: &str = "_summary.json";

/// Default output directory name.
pub const OUTPUT_DIR_DEFAULT: &str = "output";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_file_constant() {
        assert_eq!(SUMMARY_FILE, "_summary.json");
    }

    #[test]
    fn test_output_dir_default() {
        assert_eq!(OUTPUT_DIR_DEFAULT, "output");
    }
}
