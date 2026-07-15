use crb_agents::{agent::AgentEntry, prompts::PromptLibrary};
use crb_shared::diff::Diff;
use crb_types::wrappers::WrappedData;
use tracing::{debug, warn};

const DEFAULT_MAX_FILES: usize = 3;
const DEFAULT_MAX_LINES: usize = 200;

/// Languages that always trigger the full 4-agent panel regardless of PR size.
const FULL_PANEL_LANGUAGES: &[&str] = &[
    ".go", ".rs", ".java", ".cpp", ".cc", ".cxx", ".c", ".ts", ".tsx",
];

/// Determine whether the given diff touches any of the full-panel languages.
///
/// Scans each `diff --git` line for file paths ending with one of the `FULL_PANEL_LANGUAGES` extensions.
//TODO: Rewrite to use `Diff` struct instead.
pub fn diff_touches_full_panel_languages(diff: &str) -> bool {
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            // Format: diff --git a/path b/path
            // We extract the "b/" path
            if let Some(bpath) = line.rsplit(' ').next() {
                let bpath = bpath.trim();
                if let Some(ext_start) = bpath.rfind('.') {
                    let ext = &bpath[ext_start..];
                    if FULL_PANEL_LANGUAGES.contains(&ext) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Determine which roles to use for a given diff, based on the available roles and the diff size.
///
/// If the diff is small enough, and a GEN agent is available, it will return only the GEN agent.
/// Otherwise, it will return all available roles that are not generalist agents.
pub fn get_agents_for_diff(
    #[allow(unused_variables)] diff: &Diff, // This is only used when the `exp16_adaptive_agents` feature is enabled
    selected_agents: &[&'static AgentEntry],
) -> Vec<&'static AgentEntry> {
    let library = PromptLibrary::get_instance();

    let mut selected_agents = selected_agents.to_vec();
    if selected_agents.is_empty() {
        warn!(
            "Adaptive dispatch: no selected agents provided; using all available roles from `PromptLibrary`."
        );

        selected_agents = library.agents();
    }

    if should_use_single_agent(diff, DEFAULT_MAX_FILES, DEFAULT_MAX_LINES) {
        if let Some(generalist) = library.generalist() {
            use tracing::info;

            if !selected_agents.iter().any(|agent| agent.generalist_agent) {
                warn!(
                    "Adaptive dispatch: small PR detected, but generalist agent is not in available roles; falling back to full panel"
                );
            }

            info!("Adaptive dispatch: small PR detected, using single generalist agent");
            return vec![generalist];
        }

        warn!(
            "Adaptive Dispatch: small PR detected, but no generalist agent found; falling back to full panel"
        );
    }

    selected_agents
        .iter()
        .filter(|agent| !agent.generalist_agent)
        .copied()
        .collect()
}

/// Decide whether a single GEN agent should be used for this diff.
///
/// Returns `true` (single GEN agent) when:
/// - File count ≤ `max_files`
/// - Total changed lines ≤ `max_lines`
/// - The diff does NOT touch any full-panel languages
///
/// Returns `false` (full 4-agent panel) otherwise.
pub fn should_use_single_agent(diff: &Diff, max_files: usize, max_lines: usize) -> bool {
    let file_count = diff.sections.len();
    let line_count = diff.sections
        .iter()
        .flat_map(|s| s.body.lines())
        .filter(|line| line.starts_with('+') || line.starts_with('-'))
        .count();

    debug!(
        "Adaptive dispatch: {} files, {} changed lines (threshold: {} files / {} lines)",
        file_count, line_count, max_files, max_lines,
    );

    if diff_touches_full_panel_languages(diff.get()) {
        debug!("Adaptive dispatch: full panel forced (diff touches safety-override language)");
        return false;
    }

    if file_count <= max_files && line_count <= max_lines {
        debug!("Adaptive dispatch: using single GEN agent");
        return true;
    }

    debug!("Adaptive dispatch: using full agent panel");
    false
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Build a minimal single-hunk diff for the given file path and content.
    /// Content should include the `-` and `+` prefix lines (e.g. "-old\n+new\n").
    fn minimal_diff(file_path: &str, content: &str) -> String {
        format!(
            "\
diff --git a/{fp} b/{fp}
--- a/{fp}
+++ b/{fp}
@@ -1 +1 @@
{content}",
            fp = file_path,
            content = content
        )
    }

    #[test]
    fn test_diff_touches_full_panel_languages_no_match() {
        let diff = "\
diff --git a/src/main.py b/src/main.py
diff --git a/README.md b/README.md
";
        assert!(!diff_touches_full_panel_languages(diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_rust() {
        let diff = minimal_diff("src/main.rs", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_typescript() {
        let diff = minimal_diff("src/foo.ts", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_go() {
        let diff = minimal_diff("server.go", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_java() {
        let diff = minimal_diff("Main.java", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_cpp() {
        let diff = minimal_diff("main.cpp", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_should_use_single_agent_small_pr() {
        let raw = minimal_diff("README.md", "-old\n+new\n");
        let diff = Diff::new(raw);
        assert!(should_use_single_agent(
            &diff,
            DEFAULT_MAX_FILES,
            DEFAULT_MAX_LINES
        ));
    }

    #[test]
    fn test_should_use_single_agent_too_many_files() {
        let file_count = 4;
        let raw = (0..file_count)
            .map(|i| {
                let fname = format!("a{}.txt", i);
                minimal_diff(&fname, "-old\n+new\n")
            })
            .collect::<Vec<_>>()
            .join("");
        let diff = Diff::new(raw);
        assert!(!should_use_single_agent(
            &diff,
            DEFAULT_MAX_FILES,
            DEFAULT_MAX_LINES
        ));
    }

    #[test]
    fn test_should_use_single_agent_too_many_lines() {
        let raw = "\
diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1,100 +1,300 @@
"
        .to_string()
            + &(0..250)
                .map(|i| format!("+line_{}\n", i))
                .collect::<String>();
        let diff = Diff::new(raw);
        assert!(!should_use_single_agent(
            &diff,
            DEFAULT_MAX_FILES,
            DEFAULT_MAX_LINES
        ));
    }

    #[test]
    fn test_should_use_single_agent_safety_override_rust() {
        let raw = minimal_diff("src/main.rs", "-old\n+new\n");
        let diff = Diff::new(raw);
        assert!(!should_use_single_agent(
            &diff,
            DEFAULT_MAX_FILES,
            DEFAULT_MAX_LINES
        ));
    }

    #[test]
    fn test_should_use_single_agent_safety_override_go() {
        let raw = minimal_diff("server.go", "-old\n+new\n");
        let diff = Diff::new(raw);
        assert!(!should_use_single_agent(
            &diff,
            DEFAULT_MAX_FILES,
            DEFAULT_MAX_LINES
        ));
    }
}
