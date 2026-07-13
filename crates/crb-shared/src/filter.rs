use std::{collections::HashMap, sync::LazyLock};

use tracing::warn;

/// Groups of file path patterns whose entire diff sections are always stripped.
static FILTERED_FILE_PATTERNS: LazyLock<Vec<FilteredPath>> = LazyLock::new(|| {
    vec![
        FilteredPath {
            name: "lock",
            description: "Lock files",
            paths: &[
                "pnpm-lock.yaml",
                "package-lock.json",
                "yarn.lock",
                "Cargo.lock",
                "Gemfile.lock",
                "composer.lock",
                "Pipfile.lock",
                "poetry.lock",
                "bun.lockb",
                "deno.lock",
                "flake.lock",
            ],
        },
        FilteredPath {
            name: "vendor",
            description: "Vendor / dependency directories",
            paths: &["/node_modules/", "/vendor/", "/Pods/"],
        },
        FilteredPath {
            name: "build",
            description: "Build output directories",
            paths: &["/dist/", "/build/", "/.next/", "/.nuxt/"],
        },
        FilteredPath {
            name: "minified",
            description: "Minified assets",
            paths: &[".min.js", ".min.css"],
        },
        FilteredPath {
            name: "map",
            description: "Source maps",
            paths: &[".map"],
        },
        FilteredPath {
            name: "coverage",
            description: "Coverage reports",
            paths: &["/coverage/", "/htmlcov/"],
        },
        FilteredPath {
            name: "snapshot",
            description: "Test snapshots",
            paths: &["__snapshots__/"],
        },
    ]
});

pub struct FilteredPath {
    // Canonical name of the filter category.
    pub name: &'static str,

    /// Path patterns that match files for filtering.
    pub paths: &'static [&'static str],

    /// Human-readable description of the filter category.
    pub description: &'static str,
}

impl FilteredPath {
    /// Check whether the given path matches any of the filter patterns.
    pub fn is_filtered(path: &str) -> bool {
        FILTERED_FILE_PATTERNS.iter().any(|pat| pat.matches(path))
    }

    /// Check whether the given path matches any of the filter patterns.
    pub fn matches(&self, path: &str) -> bool {
        self.paths.iter().any(|p| {
            if path.contains(p) || path.ends_with(p) {
                return true;
            }

            // For patterns starting with '/', also check relative paths
            if let Some(stripped) = p.strip_prefix('/') {
                if path.contains(stripped) || path.starts_with(stripped) || path.ends_with(stripped)
                {
                    return true;
                }
            }

            false
        })
    }
}

/// Count the categories of filtered files.
#[derive(Default)]
pub struct FilterCounts {
    /// Number of files filtered by each category.
    ///
    /// Indexed by the canonical name of the filter category.
    pub counter: HashMap<&'static str, usize>,
}

impl FilterCounts {
    /// Check whether the given path matches any of the filter patterns,
    /// and if so, increment the counter for that category.
    pub fn check_and_add(&mut self, path: &str) -> bool {
        if FilteredPath::is_filtered(path) {
            self.add(path);
            return true;
        }

        false
    }

    pub fn total(&self) -> usize {
        self.counter.values().sum()
    }

    pub fn add(&mut self, path: &str) {
        let Some(category) = FILTERED_FILE_PATTERNS
            .iter()
            .find(|filter| filter.matches(path))
        else {
            warn!(
                "Filtered file path {} did not match any known filter category",
                path
            );
            return;
        };

        self.counter
            .entry(category.name)
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }

    pub fn fmt_note(&self) -> String {
        if self.total() == 0 {
            return String::new();
        }

        let mut parts: Vec<String> = Vec::new();
        for filter in FILTERED_FILE_PATTERNS.iter() {
            if let Some(count) = self.counter.get(filter.name) {
                parts.push(format!("{} {}", count, filter.description));
            }
        }

        let detail = parts.join(", ");
        format!(
            "[{} files filtered: {} - see raw diff for details]",
            self.total(),
            detail
        )
    }
}
