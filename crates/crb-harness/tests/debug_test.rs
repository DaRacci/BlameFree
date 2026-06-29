#[test]
fn debug_minified_coverage() {
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
    let result = crb_harness::preprocess_diff(diff);
    println!("RESULT:\n---\n{}---\n", result);
    println!("Contains 'bundle.min.js': {}", result.contains("bundle.min.js"));
    println!("Contains 'coverage': {}", result.contains("coverage"));
    println!("Contains 'src/lib.rs': {}", result.contains("src/lib.rs"));
    println!("Contains 'filtered': {}", result.contains("filtered"));
    
    // Check what the note says
    if let Some(start) = result.find('[') {
        if let Some(end) = result[start..].find(']') {
            let note = &result[start..start+end+1];
            println!("FILTER NOTE: {:?}", note);
        }
    }
}
