//! Adaptive agent dispatch (EXP-016) — decide single vs. full panel agents.

/// Languages that always trigger the full 4-agent panel regardless of PR size.
const FULL_PANEL_LANGUAGES: &[&str] = &[
    ".go", ".rs", ".java", ".cpp", ".cc", ".cxx", ".c", ".ts", ".tsx",
];

/// Determine whether the given diff touches any of the full-panel languages.
///
/// Scans each `diff --git` line for file paths ending with one of the
/// [`FULL_PANEL_LANGUAGES`] extensions (Go, Rust, Java, C++, C, TypeScript).
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

/// Parse a unified diff to count the number of changed files.
pub fn count_diff_files(diff: &str) -> usize {
    diff.lines()
        .filter(|line| line.starts_with("diff --git "))
        .count()
}

/// Parse a unified diff to count the total number of changed lines (additions
/// and deletions, excluding `---`/`+++` hunk headers and `diff --git` lines).
pub fn count_diff_lines(diff: &str) -> usize {
    diff.lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Count lines starting with + or - but not +++/---
            (trimmed.starts_with('+') || trimmed.starts_with('-'))
                && !trimmed.starts_with("+++")
                && !trimmed.starts_with("---")
        })
        .count()
}

/// Decide whether a single GEN agent should be used for this diff.
///
/// Returns `true` (single GEN agent) when:
/// - File count ≤ `max_files`
/// - Total changed lines ≤ `max_lines`
/// - The diff does NOT touch any full-panel languages
///
/// Returns `false` (full 4-agent panel) otherwise.
#[allow(clippy::cognitive_complexity)]
pub fn should_use_single_agent(diff: &str, max_files: usize, max_lines: usize) -> bool {
    let file_count = count_diff_files(diff);
    let line_count = count_diff_lines(diff);

    tracing::debug!(
        "Adaptive dispatch: {} files, {} changed lines (threshold: {} files / {} lines)",
        file_count,
        line_count,
        max_files,
        max_lines,
    );

    // Safety override: full panel for complex languages
    if diff_touches_full_panel_languages(diff) {
        tracing::debug!(
            "Adaptive dispatch: full panel forced (diff touches safety-override language)"
        );
        return false;
    }

    // Small PR: single GEN agent
    if file_count <= max_files && line_count <= max_lines {
        tracing::debug!("Adaptive dispatch: using single GEN agent (small PR)");
        return true;
    }

    tracing::debug!("Adaptive dispatch: using full 4-agent panel (complex PR)");
    false
}
