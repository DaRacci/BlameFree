use std::mem;

use crb_cache::traits::CacheKey;
use crb_types::wrappers::WrappedData;
use sha2::{Digest, Sha256};

use crate::filter::FilterCounts;

const CONTEXT_LINES: usize = 1;

#[derive(Debug, Default)]
pub struct Diff {
    /// The original raw diff string.
    pub raw: String,

    /// The parsed diff sections, one per file.
    pub sections: Vec<DiffSection>,

    /// Additional notes about the diff that may be added during processing.
    ///
    /// These notes will be provided to the LLM prompt to inform the model about any filtering or modifications made to the diff.
    pub notes: Vec<String>,
}

impl CacheKey for Diff {
    fn cache_key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.raw.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[derive(Debug)]
pub struct DiffSection {
    /// The file path of the diff section, extracted from the `diff --git` header.
    pub path: String,

    /// The header of the diff section, including `diff --git` and file metadata.
    pub header: String,

    /// The body of the diff section, including hunk headers and changed lines.
    pub body: String,
}

impl WrappedData for Diff {
    fn get(&self) -> &str {
        &self.raw
    }
}

impl Diff {
    pub fn new(raw: String) -> Self {
        let sections = DiffSection::get_hunks(&raw);
        Self {
            raw,
            sections,
            ..Default::default()
        }
    }
}

impl DiffSection {
    /// Split a raw unified diff into per-file sections.
    ///
    /// Each section begins with `diff --git a/...`
    /// and extends until the next `diff --git` or end-of-string.
    pub fn get_hunks(diff: &str) -> Vec<Self> {
        let mut sections: Vec<Self> = Vec::new();
        let mut current_header = String::new();
        let mut current_body = String::new();

        for line in diff.lines() {
            if line.starts_with("diff --git ") {
                if !current_header.is_empty() || !current_body.is_empty() {
                    sections.push(DiffSection {
                        path: parse_diff_git_path(&current_header).unwrap(),
                        body: mem::take(&mut current_body),
                        header: mem::take(&mut current_header),
                    });
                }
                current_header = line.to_string();
            } else {
                if !current_header.is_empty() {
                    if !current_body.is_empty() {
                        current_body.push('\n');
                    }
                    current_body.push_str(line);
                }
            }
        }

        // We have reached the end of the diff; if we have a current section, push it.
        if !current_header.is_empty() || !current_body.is_empty() {
            sections.push(DiffSection {
                path: parse_diff_git_path(&current_header).unwrap(),
                body: current_body,
                header: current_header,
            });
        }

        sections
    }
}

/// Extract the file path from a `diff --git a/path b/path` header line.
fn parse_diff_git_path(line: &str) -> Option<String> {
    let line = line.trim();
    let rest = line.strip_prefix("diff --git a/")?;
    let end = rest.find(" b/")?;
    Some(rest[..end].to_string())
}

/// Strip diff metadata and reduce context to -U1 for unified diffs.
///
/// 1. Reduces context to -U1: keeps 1 context line before/after changed lines
/// 2. Strips `diff --git` headers
/// 3. Strips `index` lines
/// 4. Strips trailing hunk context text (after `@@` line-count portion)
/// 5. Keeps `--- a/path`, `+++ b/path`, `new file mode`, `deleted file mode`, `@@` hunk headers
pub fn strip_diff_metadata(diff: &str) -> String {
    let mut result = Vec::new();
    let mut current_hunk_lines: Vec<&str> = Vec::new();
    let mut in_hunk = false;

    for line in diff.lines() {
        // Skip diff --git and index lines
        if line.starts_with("diff --git") || line.starts_with("index ") {
            continue;
        }

        if line.starts_with("@@ ") && line.contains(" @@") {
            // Start of a new hunk; flush previous hunk if any
            if in_hunk && !current_hunk_lines.is_empty() {
                flush_hunk(&current_hunk_lines, &mut result, CONTEXT_LINES);
                current_hunk_lines.clear();
            }
            in_hunk = true;
            current_hunk_lines.push(line);
        } else if in_hunk {
            // Inside a hunk — collect body lines
            current_hunk_lines.push(line);
        } else {
            // Before the first hunk — pass through (---, +++, etc.)
            result.push(line.to_string());
        }
    }

    // Flush the last hunk
    if !current_hunk_lines.is_empty() {
        flush_hunk(&current_hunk_lines, &mut result, CONTEXT_LINES);
    }

    result.join("\n")
}

/// Filter a raw diff to remove noise files.
/// Returns the filtered diff with a summary note at the top if any files were removed.
pub fn preprocess_diff(raw_diff: &mut Diff) {
    filter_files(raw_diff);
    // strip_diff_metadata(&filtered);
}

// strip trailing text after the @@ line-count portion
// Header format: @@ -a,b +c,d @@ optional text
fn strip_hunk_header_text(header: &str) -> String {
    let parts: Vec<&str> = header.split("@@").collect();
    // split on "@@" gives: ["", " -a,b +c,d ", " optional text"]
    // We want: @@ + middle + @@
    if parts.len() >= 3 {
        format!("@@{}@@", parts[1])
    } else {
        header.to_string()
    }
}

// flush the current hunk with -U`context` reduction
fn flush_hunk(hunk_lines: &[&str], output: &mut Vec<String>, context: usize) {
    if hunk_lines.is_empty() {
        return;
    }

    // Split: first line is the @@ header, rest are body lines
    let header = hunk_lines[0];
    let body = &hunk_lines[1..];

    let first_changed = body
        .iter()
        .position(|l| l.starts_with('+') || l.starts_with('-'));
    let last_changed = body
        .iter()
        .rposition(|l| l.starts_with('+') || l.starts_with('-'));

    let (Some(first), Some(last)) = (first_changed, last_changed) else {
        // No changed lines; keep hunk as-is
        output.push(header.to_string());
        for line in body {
            output.push(line.to_string());
        }
        return;
    };

    // Determine start: context line(s) before first changed, or 0 if not enough
    let start = if first > 0 { first - context } else { 0 };
    // Determine end: context line(s) after last changed
    let end = if last + (context + 1) < body.len() {
        last + (context + 1)
    } else {
        body.len()
    };

    let stripped_header = strip_hunk_header_text(header);
    output.push(stripped_header);

    for line in &body[start..end] {
        output.push(line.to_string());
    }
}

/// Filter out files matching `FILTERED_FILE_PATTERNS` from a raw diff.
///
/// Returns the filtered diff with a summary note at the top.
fn filter_files(diff: &mut Diff) {
    let mut counts = FilterCounts::default();

    diff.sections
        .retain(|section| !counts.check_and_add(&section.path));
    diff.notes.push(counts.fmt_note());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_minified_coverage() {
        let mut diff =
            Diff::new(include_str!("../tests/fixtures/mixed_real_and_generated.diff").to_string());

        preprocess_diff(&mut diff);
        info!("RESULT:\n---\n{}---\n", diff.raw);
        info!(
            "Contains 'bundle.min.js': {}",
            diff.raw.contains("bundle.min.js")
        );
        info!("Contains 'coverage': {}", diff.raw.contains("coverage"));
        info!("Contains 'src/lib.rs': {}", diff.raw.contains("src/lib.rs"));
        info!("Contains 'filtered': {}", diff.raw.contains("filtered"));

        // Check what the note says
        if let Some(start) = diff.raw.find('[') {
            if let Some(end) = diff.raw[start..].find(']') {
                let note = &diff.raw[start..start + end + 1];
                info!("FILTER NOTE: {:?}", note);
            }
        }
    }

    #[test]
    fn filter_removes_pnpm_lock() {
        let mut diff = Diff::new(include_str!("../tests/fixtures/pnpm_lock.diff").to_string());
        preprocess_diff(&mut diff);
        assert!(
            diff.sections.iter().any(|s| s.path == "src/lib.rs"),
            "should contain src/lib.rs"
        );
        assert!(
            !diff.sections.iter().any(|s| s.path == "pnpm-lock.yaml"),
            "should NOT contain pnpm-lock"
        );
        assert!(
            diff.notes.iter().any(|n| n.contains("filtered")),
            "should contain a filter note"
        );
    }

    #[test]
    fn filter_removes_node_modules() {
        let mut diff = Diff::new(include_str!("../tests/fixtures/node_modules.diff").to_string());
        preprocess_diff(&mut diff);
        assert!(
            diff.sections.iter().any(|s| s.path == "src/main.rs"),
            "should contain real file"
        );
        assert!(
            !diff
                .sections
                .iter()
                .any(|s| s.path == "node_modules/pkg/index.js"),
            "should remove vendor"
        );
        assert!(
            diff.notes.iter().any(|n| n.contains("filtered")),
            "should have filter note"
        );
    }

    #[test]
    fn filter_removes_minified_and_coverage() {
        let mut diff =
            Diff::new(include_str!("../tests/fixtures/mixed_real_and_generated.diff").to_string());
        preprocess_diff(&mut diff);
        assert!(
            !diff.sections.iter().any(|s| s.path == "dist/bundle.min.js"),
            "should remove minified"
        );
        assert!(
            !diff
                .sections
                .iter()
                .any(|s| s.path == "coverage/report.html"),
            "should remove coverage diff"
        );
        assert!(
            diff.sections.iter().any(|s| s.path == "src/lib.rs"),
            "should keep real file"
        );
        assert!(
            diff.notes.iter().any(|n| n.contains("filtered")),
            "should have filter note"
        );
    }

    #[test]
    fn filter_empty_diff_no_note() {
        let mut diff = Diff::new("".to_string());
        preprocess_diff(&mut diff);
        assert!(diff.sections.is_empty());
        // preprocess_diff always calls filter_files which adds a note, but
        // with no sections it adds an empty note
        assert!(diff.notes.is_empty() || diff.notes.iter().all(|n| n.is_empty()));
    }

    #[test]
    fn filter_no_filterable_files_no_note() {
        let mut diff = Diff::new(include_str!("../tests/fixtures/only_src_lib.diff").to_string());
        preprocess_diff(&mut diff);
        assert!(
            diff.sections.iter().any(|s| s.path == "src/lib.rs"),
            "should contain the only section"
        );
        assert!(
            diff.notes.is_empty() || diff.notes.iter().all(|n| n.trim().is_empty() || n == "[]"),
            "no filter note when nothing filtered"
        );
    }

    #[test]
    fn filter_multiple_categories_noted() {
        let mut diff =
            Diff::new(include_str!("../tests/fixtures/multiple_categories.diff").to_string());
        preprocess_diff(&mut diff);
        assert!(
            diff.notes.iter().any(|n| n.contains("filtered")),
            "should note filtering"
        );
        assert!(
            diff.sections.iter().any(|s| s.path == "src/lib.rs"),
            "should keep real code"
        );
    }

    mod strip_diff_metadata_tests {
        use super::strip_diff_metadata;

        #[test]
        fn reduces_three_line_context_to_one() {
            let diff = include_str!("../tests/fixtures/three_line_context.diff");
            let result = strip_diff_metadata(diff);
            let lines: Vec<&str> = result.lines().collect();
            // Should keep --- and +++ lines
            assert_eq!(lines[0], "--- a/src/main.rs");
            assert_eq!(lines[1], "+++ b/src/main.rs");
            // Header should be stripped of trailing context
            assert_eq!(lines[2], "@@ -5,7 +5,7 @@");
            // Should have: (---, +++, header + 1 before + 2 changed + 1 after) = 7 lines
            assert_eq!(lines.len(), 7, "expected 7 lines total");
            // Before context (only 1 line instead of 3)
            assert_eq!(lines[3], " // line 3");
            // Changed lines
            assert!(lines[4].starts_with('-'));
            assert!(lines[5].starts_with('+'));
            // After context (only 1 line instead of 3)
            assert_eq!(lines[6], " // line 5");
        }

        #[test]
        fn strips_diff_git_and_index_lines() {
            let diff = include_str!("../tests/fixtures/diff_git_index.diff");
            let result = strip_diff_metadata(diff);
            assert!(
                !result.contains("diff --git"),
                "diff --git should be stripped"
            );
            assert!(!result.contains("index "), "index lines should be stripped");
            // Should still contain --- and +++
            assert!(result.contains("--- a/src/main.rs"), "--- should be kept");
            assert!(result.contains("+++ b/src/main.rs"), "+++ should be kept");
        }

        #[test]
        fn strips_hunk_header_context_text() {
            let diff = include_str!("../tests/fixtures/hunk_header_text.diff");
            let result = strip_diff_metadata(diff);
            let lines: Vec<&str> = result.lines().collect();
            // Should keep --- and +++
            assert_eq!(lines[0], "--- a/src/lib.rs");
            assert_eq!(lines[1], "+++ b/src/lib.rs");
            // Header should have stripped trailing "pub fn compute(...)"
            assert_eq!(lines[2], "@@ -10,6 +10,8 @@");
            assert!(result.contains("old_path") || result.contains("new_path"));
        }
    }
}
