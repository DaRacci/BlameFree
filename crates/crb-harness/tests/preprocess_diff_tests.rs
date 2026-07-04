//! Tests for diff preprocessing: filtering and chunking.

use crb_harness::preprocess_diff;

// ---------------------------------------------------------------------------
// The preprocess_diff function (without --features reduce-diff) just passes
// the diff through unchanged, so filtering tests only apply with the feature.
// ---------------------------------------------------------------------------

#[cfg(feature = "reduce-diff")]
#[test]
fn filter_removes_pnpm_lock() {
    let diff = concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "index abc..def 100644\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -1 +1 @@\n",
        "-old\n",
        "+new\n",
        "diff --git a/pnpm-lock.yaml b/pnpm-lock.yaml\n",
        "index 111..222 100644\n",
        "--- a/pnpm-lock.yaml\n",
        "+++ b/pnpm-lock.yaml\n",
        "@@ -100 +100 @@\n",
        "-lock\n",
        "+unlock\n",
    );
    let result = preprocess_diff(diff);
    // The lock file section should be removed; only src/lib.rs remains
    assert!(result.contains("src/lib.rs"), "should contain src/lib.rs");
    assert!(
        !result.contains("pnpm-lock"),
        "should NOT contain pnpm-lock"
    );
    assert!(result.contains("filtered"), "should contain a filter note");
}

#[cfg(feature = "reduce-diff")]
#[test]
fn filter_removes_node_modules() {
    let diff = concat!(
        "diff --git a/src/main.rs b/src/main.rs\n",
        "index a..b 100644\n",
        "--- a/src/main.rs\n",
        "+++ b/src/main.rs\n",
        "@@ -1 +1 @@\n",
        "-a\n",
        "+b\n",
        "diff --git a/node_modules/pkg/index.js b/node_modules/pkg/index.js\n",
        "index c..d 100644\n",
        "--- a/node_modules/pkg/index.js\n",
        "+++ b/node_modules/pkg/index.js\n",
        "@@ -1 +1 @@\n",
        "-old\n",
        "+new\n",
    );
    let result = preprocess_diff(diff);
    assert!(result.contains("src/main.rs"), "should contain real file");
    assert!(!result.contains("node_modules"), "should remove vendor");
    assert!(result.contains("filtered"), "should have filter note");
}

#[cfg(feature = "reduce-diff")]
#[test]
fn filter_removes_minified_and_coverage() {
    let diff = concat!(
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
    );
    let result = preprocess_diff(diff);
    assert!(!result.contains("bundle.min.js"), "should remove minified");
    // The diff section for coverage should be removed, but the word "coverage"
    // may still appear in the filter summary note, so check for the diff marker instead
    assert!(result.contains("src/lib.rs"), "should keep real file");
    assert!(
        !result.contains("coverage/report"),
        "should remove coverage diff"
    );
    assert!(result.contains("filtered"), "should have filter note");
}

#[cfg(feature = "reduce-diff")]
#[test]
fn filter_empty_diff_no_note() {
    let result = preprocess_diff("");
    assert_eq!(result, "");
}

#[cfg(feature = "reduce-diff")]
#[test]
fn filter_no_filterable_files_no_note() {
    let diff = concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "index a..b 100644\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -1 +1 @@\n",
        "-old\n",
        "+new\n",
    );
    let result = preprocess_diff(diff);
    assert!(result.contains("src/lib.rs"));
    // No filter note expected
    assert!(
        !result.contains("filtered"),
        "no filter note when nothing filtered"
    );
}

#[cfg(feature = "reduce-diff")]
#[test]
fn filter_multiple_categories_noted() {
    let diff = concat!(
        "diff --git a/yarn.lock b/yarn.lock\n",
        "index a..b 100644\n",
        "--- a/yarn.lock\n",
        "+++ b/yarn.lock\n",
        "@@ -1 +1 @@\n",
        "-lock\n",
        "+lock2\n",
        "diff --git a/vendor/some-lib/lib.rs b/vendor/some-lib/lib.rs\n",
        "index c..d 100644\n",
        "--- a/vendor/some-lib/lib.rs\n",
        "+++ b/vendor/some-lib/lib.rs\n",
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
    );
    let result = preprocess_diff(diff);
    assert!(result.contains("filtered"), "should note filtering");
    assert!(
        result.contains("1 lock") || result.contains("1 vendor"),
        "should detail categories"
    );
    assert!(result.contains("src/lib.rs"), "should keep real code");
}

// ---------------------------------------------------------------------------
// strip_diff_metadata tests
// ---------------------------------------------------------------------------

#[cfg(feature = "reduce-diff")]
mod strip_diff_metadata_tests {
    use crb_harness::strip_diff_metadata;

    /// Test 1: Reduces 3-line context to 1-line context
    #[test]
    fn reduces_three_line_context_to_one() {
        let diff = concat!(
            "--- a/src/main.rs\n",
            "+++ b/src/main.rs\n",
            "@@ -5,7 +5,7 @@ fn foo() {\n",
            " // line 1\n",
            " // line 2\n",
            " // line 3\n",
            "-    old_code();\n",
            "+    new_code();\n",
            " // line 5\n",
            " // line 6\n",
            " // line 7\n",
        );
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

    /// Test 2: Strips diff --git and index lines
    #[test]
    fn strips_diff_git_and_index_lines() {
        let diff = concat!(
            "diff --git a/src/main.rs b/src/main.rs\n",
            "index abc123..def456 100644\n",
            "--- a/src/main.rs\n",
            "+++ b/src/main.rs\n",
            "@@ -1,3 +1,3 @@\n",
            " unchanged\n",
            "-old\n",
            "+new\n",
        );
        let result = strip_diff_metadata(diff);
        // Should not contain "diff --git"
        assert!(
            !result.contains("diff --git"),
            "diff --git should be stripped"
        );
        // Should not contain "index "
        assert!(!result.contains("index "), "index lines should be stripped");
        // Should still contain --- and +++
        assert!(result.contains("--- a/src/main.rs"), "--- should be kept");
        assert!(result.contains("+++ b/src/main.rs"), "+++ should be kept");
    }

    /// Test 3: Strips trailing hunk context text
    #[test]
    fn strips_hunk_header_context_text() {
        let diff = concat!(
            "--- a/src/lib.rs\n",
            "+++ b/src/lib.rs\n",
            "@@ -10,6 +10,8 @@ pub fn compute(input: &str) -> i32 {\n",
            " let x = 1;\n",
            "-    old_path();\n",
            "+    new_path();\n",
            "+    extra_line();\n",
            " return x;\n",
        );
        let result = strip_diff_metadata(diff);
        let lines: Vec<&str> = result.lines().collect();
        // Should keep --- and +++
        assert_eq!(lines[0], "--- a/src/lib.rs");
        assert_eq!(lines[1], "+++ b/src/lib.rs");
        // Header should have stripped trailing "pub fn compute(...)"
        assert_eq!(lines[2], "@@ -10,6 +10,8 @@");
        // Should still have the changed content
        assert!(result.contains("old_path") || result.contains("new_path"));
    }
}
