/// Extract the file path from a `diff --git a/path b/path` header line.
fn parse_diff_git_path(line: &str) -> Option<&str> {
    let line = line.trim();
    let rest = line.strip_prefix("diff --git a/")?;
    let end = rest.find(" b/")?;
    Some(&rest[..end])
}

/// Split a raw unified diff into per-file sections,
/// returning the header separator and section body for each.
///
/// Each section begins with `diff --git a/...`
/// and extends until the next `diff --git` or end-of-string.
fn split_diff_sections(diff: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
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
            sections.push((
                std::mem::take(&mut current_header),
                std::mem::take(&mut current_body),
            ));
        }
        current_header = line.to_string();
    }

    // We have reached the end of the diff; if we have a current section, push it.
    if !current_header.is_empty() || !current_body.is_empty() {
        sections.push((current_header, current_body));
    }

    sections
}

/// Strip diff metadata and reduce context to -U1 for unified diffs.
///
/// 1. Reduces context to -U1: keeps 1 context line before/after changed lines
/// 2. Strips `diff --git` headers
/// 3. Strips `index` lines
/// 4. Strips trailing hunk context text (after `@@` line-count portion)
/// 5. Keeps `--- a/path`, `+++ b/path`, `new file mode`, `deleted file mode`, `@@` hunk headers
#[cfg(feature = "reduce-diff")]
pub fn strip_diff_metadata(diff: &str) -> String {
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
            flush_hunk(&current_hunk_lines, &mut result);
            current_hunk_lines.clear();
        }

        in_hunk = true;
        current_hunk_lines.push(line);
    }

    if !current_hunk_lines.is_empty() {
        flush_hunk(&current_hunk_lines, &mut result);
    }

    result.join("\n")
}

/// Filter a raw diff to remove noise files.
/// Returns the filtered diff with a summary note at the top if any files were removed.
pub fn preprocess_diff(raw_diff: &str) -> String {
    #[cfg(feature = "reduce-diff")]
    {
        use crate::filter::filter_files;

        let filtered = filter_files(raw_diff);
        strip_diff_metadata(&filtered)
    }
    #[cfg(not(feature = "reduce-diff"))]
    {
        raw_diff.to_string()
    }
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

// flush the current hunk with -U1 reduction
fn flush_hunk(hunk_lines: &[&str], output: &mut Vec<String>) {
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

    // Determine start: 1 context line before first changed, or 0 if not enough
    let start = if first > 0 { first - 1 } else { 0 };
    // Determine end: 1 context line after last changed
    let end = if last + 2 < body.len() {
        last + 2
    } else {
        body.len()
    };

    let stripped_header = strip_hunk_header_text(header);
    output.push(stripped_header);

    for line in &body[start..end] {
        output.push(line.to_string());
    }
}
