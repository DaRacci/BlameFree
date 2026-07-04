use regex::Regex;

/// Shared trait for patterns based scanners.
pub trait PatternProvider {
    /// Returns the name of the pattern provider.
    fn name(&self) -> &'static str;

    /// Returns the raw regex patterns.
    fn patterns(&self) -> &[&'static str];
}

pub fn make_pattern_list<T>(patterns: &Vec<T>) -> Vec<(&'static str, &'static str, Regex)>
where
    T: PatternProvider,
{
    let mut result = Vec::new();
    for cat in patterns.iter() {
        for &pat in cat.patterns().iter() {
            if let Ok(re) = Regex::new(&format!("(?i){}", pat)) {
                result.push((cat.name(), pat, re));
            }
        }
    }
    result
}

/// Match a finding against the patterns provided in `pattern_list`.
///
/// Returns `(category_name, matching_pattern)` if a match is found.
pub fn has_pattern(
    finding_text: &str,
    evidence: &str,
    pattern_list: &Vec<(&'static str, &'static str, Regex)>,
) -> Option<(&'static str, &'static str)> {
    let combined = format!("{} {}", finding_text, evidence);
    for (category_name, pattern, re) in pattern_list.iter() {
        if re.is_match(&combined) {
            return Some((category_name, pattern));
        }
    }
    None
}
