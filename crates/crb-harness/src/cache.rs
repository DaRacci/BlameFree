//! LLM interaction cache — captures every prompt/response to disk for debugging,
//! regression testing, and deterministic replay.
//!
//! When `--cache-dir` is set, each PR gets its own subdirectory at
//! `{cache_dir}/{pr_key}/` containing:
//! - `agent_{role}_prompt.txt` / `agent_{role}_response.txt` — agent LLM calls
//! - `judge_calls.jsonl` — each judge call (golden, finding, verdict)
//! - `metadata.json` — PR info, timestamps, model, token counts
//!
//! At the end of a run, `_summary.json` is written with aggregate stats.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use crb_consensus::CacheBackend;

/// Result type alias for cache operations.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Thread-safe cache for LLM interactions.
pub struct LlmCache {
    dir: PathBuf,
    pr_key: String,
    call_index: Mutex<usize>,
}

impl LlmCache {
    /// Create a new cache instance for the given PR.
    ///
    /// Creates the directory `{base}/{pr_key}/` if it doesn't exist.
    /// The `pr_key` is sanitized to only contain alphanumeric characters,
    /// hyphens, and underscores.
    pub fn new(base: &Path, pr_key: &str) -> Result<Self> {
        let dir = base.join(sanitize(pr_key));
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            pr_key: pr_key.to_string(),
            call_index: Mutex::new(0),
        })
    }

    /// The directory path for this PR's cache.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Save an agent prompt+response pair.
    ///
    /// Writes `agent_{role}_prompt.txt` and `agent_{role}_response.txt`.
    /// If a file for the same role already exists, it is overwritten.
    pub fn save_agent(&self, role: &str, prompt: &str, response: &str) -> Result<()> {
        std::fs::write(self.dir.join(format!("agent_{}_prompt.txt", role)), prompt)?;
        std::fs::write(
            self.dir.join(format!("agent_{}_response.txt", role)),
            response,
        )?;
        Ok(())
    }

    /// Append a judge call to the JSONL file.
    ///
    /// Each line is a JSON object with `timestamp`, `golden_comment`,
    /// `finding_message`, and `verdict` fields.
    pub fn save_judge(
        &self,
        golden: &str,
        finding: &str,
        verdict_json: &str,
    ) -> Result<()> {
        let path = self.dir.join("judge_calls.jsonl");
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let entry = serde_json::json!({
            "timestamp": format!("{:?}", SystemTime::now()),
            "golden_comment": golden,
            "finding_message": finding,
            "verdict": serde_json::from_str::<serde_json::Value>(verdict_json)
                .unwrap_or_default(),
        });
        writeln!(f, "{}", serde_json::to_string(&entry)?)?;
        Ok(())
    }

    /// Write per-PR metadata to `metadata.json`.
    ///
    /// Typical fields include PR info, timestamps, model name, and token counts.
    /// The metadata is pretty-printed for readability.
    pub fn save_metadata(&self, metadata: &serde_json::Value) -> Result<()> {
        std::fs::write(
            self.dir.join("metadata.json"),
            serde_json::to_string_pretty(metadata)?,
        )?;
        Ok(())
    }

    /// Check if the cache is active (base path is set).
    pub fn is_active(&self) -> bool {
        true
    }
}

impl CacheBackend for LlmCache {
    fn save_agent(&self, role: &str, prompt: &str, response: &str) {
        if let Err(e) = self.save_agent(role, prompt, response) {
            tracing::warn!("Cache save_agent failed: {e}");
        }
    }

    fn save_judge(&self, golden: &str, finding: &str, verdict_json: &str) {
        if let Err(e) = self.save_judge(golden, finding, verdict_json) {
            tracing::warn!("Cache save_judge failed: {e}");
        }
    }
}

/// Sanitize a name for use as a directory/file path component.
///
/// Replaces any non-alphanumeric character (except hyphens and underscores)
/// with an underscore.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_sanitize() {
        assert_eq!(sanitize("hello-world"), "hello-world");
        assert_eq!(sanitize("foo/bar:baz"), "foo_bar_baz");
        assert_eq!(sanitize("a b c"), "a_b_c");
        assert_eq!(sanitize("safe_path_123"), "safe_path_123");
    }

    #[test]
    fn test_cache_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(dir.path(), "test-pr").unwrap();
        assert!(cache.dir().exists());
        assert!(cache.dir().join("agent").parent().is_some());
    }

    #[test]
    fn test_save_agent_files() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(dir.path(), "test-pr").unwrap();

        cache
            .save_agent("SA", "system prompt...", "response...")
            .unwrap();

        let prompt_path = cache.dir().join("agent_SA_prompt.txt");
        let response_path = cache.dir().join("agent_SA_response.txt");

        assert!(prompt_path.exists());
        assert!(response_path.exists());
        assert_eq!(fs::read_to_string(prompt_path).unwrap(), "system prompt...");
        assert_eq!(fs::read_to_string(response_path).unwrap(), "response...");
    }

    #[test]
    fn test_save_judge_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(dir.path(), "test-pr").unwrap();

        cache
            .save_judge("golden comment", "finding msg", r#"{"match_":true}"#)
            .unwrap();

        let path = cache.dir().join("judge_calls.jsonl");
        assert!(path.exists());

        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("golden comment"));
        assert!(content.contains("finding msg"));
        assert!(content.contains(r#""match_":true"#));
    }

    #[test]
    fn test_save_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(dir.path(), "test-pr").unwrap();

        let metadata = serde_json::json!({
            "pr": "test-pr",
            "model": "gpt-4o",
            "timestamp": "2025-01-01T00:00:00Z"
        });
        cache.save_metadata(&metadata).unwrap();

        let path = cache.dir().join("metadata.json");
        assert!(path.exists());
        let content: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(content["pr"], "test-pr");
        assert_eq!(content["model"], "gpt-4o");
    }
}
