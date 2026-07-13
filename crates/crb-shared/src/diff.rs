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
        let stripped = strip_diff_metadata(&raw);
        let sections = DiffSection::get_hunks(&stripped);
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
            if !line.starts_with("diff --git ") {
                if !current_header.is_empty() {
                    if !current_body.is_empty() {
                        current_body.push('\n');
                    }
                    current_body.push_str(line);
                }
            }

            if !current_header.is_empty() || !current_body.is_empty() {
                sections.push(DiffSection {
                    path: parse_diff_git_path(&current_header).unwrap(),
                    body: mem::take(&mut current_body),
                    header: mem::take(&mut current_header),
                });
            }
            current_header = line.to_string();
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
fn strip_diff_metadata(diff: &str) -> String {
    let mut result = Vec::new();
    let mut current_hunk_lines: Vec<&str> = Vec::new();
    let mut in_hunk = false;

    for line in diff.lines() {
        const SKIP_PREFIXES: [&str; 2] = ["diff --git", "index "];
        if SKIP_PREFIXES.iter().any(|prefix| line.starts_with(prefix)) {
            continue;
        }

        if !(line.starts_with("@@ ") && line.contains(" @@")) {
            match in_hunk {
                // collect body lines
                true => current_hunk_lines.push(line),
                // pass through (e.g. ---, +++, new file mode, deleted file mode)
                false => result.push(line.to_string()),
            }
        }

        // Start of a new hunk; flush previous hunk if any
        if in_hunk && !current_hunk_lines.is_empty() {
            flush_hunk(&current_hunk_lines, &mut result, CONTEXT_LINES);
            current_hunk_lines.clear();
        }

        in_hunk = true;
        current_hunk_lines.push(line);
    }

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
        let diff = Diff::new(
            concat!(
                "diff --git a/dist/bundle.min.js b/dist/bundle.min.js\n",
                "index a..b 100644\n",
                "--- a/dist/bundle.min.js\n",
                "+++ b/dist/bundle.min.js\n",
                "@@ -1 +1 @@\n",
                "-var x=1\n",
                "+var x=2\n",
                "diff --git a/coverage/report.html b/coverage/report.html\n",
                "index c..d 100644\n",
                "--- a/coverage/report.html\n",
                "+++ b/coverage/report.html\n",
                "@@ -1 +1 @@\n",
                "-old\n",
                "+new\n",
                "diff --git a/src/lib.rs b/src/lib.rs\n",
                "index e..f 100644\n",
                "--- a/src/lib.rs\n",
                "+++ b/src/lib.rs\n",
                "@@ -1 +1 @@\n",
                "-fn old() {}\n",
                "+fn new() {}\n",
            )
            .to_string(),
        );

        let result = preprocess_diff(&diff);
        println!("RESULT:\n---\n{}---\n", result);
        println!(
            "Contains 'bundle.min.js': {}",
            result.contains("bundle.min.js")
        );
        println!("Contains 'coverage': {}", result.contains("coverage"));
        println!("Contains 'src/lib.rs': {}", result.contains("src/lib.rs"));
        println!("Contains 'filtered': {}", result.contains("filtered"));

        // Check what the note says
        if let Some(start) = result.find('[') {
            if let Some(end) = result[start..].find(']') {
                let note = &result[start..start + end + 1];
                println!("FILTER NOTE: {:?}", note);
            }
        }
    }
}
