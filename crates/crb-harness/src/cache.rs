//! LLM interaction cache with content-addressed indexing.
//!
//! When `--cache-dir` is set, each PR gets its own subdirectory at
//! `{cache_dir}/{pr_key}/` containing:
//!
//! Cache structure:
//! ```text
//! cache/{run_id}/
//!   {pr_key}/
//!     agents/
//!       {cache_key_hex}.agent_SA_prompt.txt
//!       {cache_key_hex}.agent_SA_response.txt
//!       ...
//!     context/
//!       {cache_key_hex}.context_prompt.txt
//!       {cache_key_hex}.context_response.txt
//!     judge/
//!       {cache_key_hex}.json
//!     index.json
//!   _summary.json
//! ```
//!
//! Content-addressed caching means every LLM interaction is keyed by a SHA256
//! hash of its inputs (prompt template hash, diff hash, model, role, rules hash).
//! If the same inputs are seen again, the cached response is reused without
//! making an API call.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use crb_consensus::CacheBackend;
use crb_judge::JudgeVerdict;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Result type alias for cache operations.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A single entry in the cache index.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// Relative path from the PR cache directory to the cached file.
    file_path: String,
    /// ISO-8601 timestamp of when the entry was created.
    timestamp: String,
    /// Model name used for this interaction.
    model: String,
    /// Optional token count (set if the provider reports it).
    tokens_used: Option<u32>,
}

/// In-memory index of all cached entries for a single PR.
/// Persisted to `index.json` after each write.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheIndex {
    /// Maps cache_key → entry metadata.
    entries: HashMap<String, CacheEntry>,
}

impl CacheIndex {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Load the index from a JSON file.
    fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!("Failed to parse cache index at {}: {e}", path.display());
                Self::new()
            }),
            Err(_) => Self::new(),
        }
    }

    /// Save the index to a JSON file.
    fn save(&self, path: &Path) {
        if let Err(e) = std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default()) {
            tracing::warn!("Failed to write cache index: {e}");
        }
    }
}

/// Thread-safe content-addressed cache for LLM interactions.
///
/// Each PR gets its own directory with an `index.json` tracking all cached
/// entries.  Cache keys are SHA256 hex digests of the inputs.
pub struct LlmCache {
    dir: PathBuf,
    _pr_key: String,
    index: Mutex<CacheIndex>,
}

impl LlmCache {
    /// Create a new cache instance for the given PR.
    ///
    /// Creates per-PR subdirectories (`agents/`, `judge/`, `context/`)
    /// and loads the existing `index.json` if present.
    pub fn new(base: &Path, pr_key: &str) -> Result<Self> {
        let sanitized = sanitize(pr_key);
        let dir = base.join(&sanitized);

        // Create subdirectories
        std::fs::create_dir_all(dir.join("agents"))?;
        std::fs::create_dir_all(dir.join("judge"))?;
        std::fs::create_dir_all(dir.join("context"))?;

        // Load existing index if any
        let index_path = dir.join("index.json");
        let index = CacheIndex::load(&index_path);

        Ok(Self {
            dir,
            _pr_key: pr_key.to_string(),
            index: Mutex::new(index),
        })
    }

    /// The directory path for this PR's cache.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    // ── SHA256 helpers ───────────────────────────────────────────────────

    /// Compute a SHA256 hex digest of the input string.
    pub fn sha256(input: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    // ── Index persistence ────────────────────────────────────────────────

    fn index_path(&self) -> PathBuf {
        self.dir.join("index.json")
    }

    /// Generate a timestamp string for the current time.
    fn now() -> String {
        format!("{:?}", SystemTime::now())
    }

    // ── Agent cache ───────────────────────────────────────────────────────

    /// Compute a content-addressed cache key for an agent LLM call.
    pub fn compute_agent_key(
        prompt_hash: &str,
        diff_hash: &str,
        model_name: &str,
        role: &str,
        rules_hash: &str,
    ) -> String {
        Self::sha256(&format!(
            "{}:{}:{}:{}:{}",
            prompt_hash, diff_hash, model_name, role, rules_hash
        ))
    }

    /// Look up a cached agent response by cache key.
    /// Returns `Some(response_text)` on hit, `None` on miss.
    pub fn lookup_agent(&self, cache_key: &str) -> Option<String> {
        let index = self.index.lock().ok()?;
        let entry = index.entries.get(cache_key)?;
        let response_path = self.dir.join(&entry.file_path);
        std::fs::read_to_string(&response_path).ok()
    }

    /// Save an agent prompt+response with its cache key and update the index.
    pub fn save_agent_cached(
        &self,
        cache_key: &str,
        role: &str,
        prompt: &str,
        response: &str,
    ) -> Result<()> {
        // Write prompt and response files
        let prompt_path = self.dir.join("agents").join(format!("{cache_key}.agent_{role}_prompt.txt"));
        let response_path = self.dir.join("agents").join(format!("{cache_key}.agent_{role}_response.txt"));

        std::fs::write(&prompt_path, prompt)?;
        std::fs::write(&response_path, response)?;

        // Update index
        let mut index = self.index.lock().map_err(|e| format!("cache index lock: {e}"))?;
        index.entries.insert(
            cache_key.to_string(),
            CacheEntry {
                file_path: format!("agents/{cache_key}.agent_{role}_response.txt"),
                timestamp: Self::now(),
                model: String::new(), // model is part of the cache key, not stored separately
                tokens_used: None,
            },
        );
        // Persist immediately
        index.save(&self.index_path());
        Ok(())
    }

    // ── Judge cache ───────────────────────────────────────────────────────

    /// Compute a content-addressed cache key for a judge LLM call.
    pub fn compute_judge_key(
        judge_prompt_hash: &str,
        finding_message: &str,
        golden_comment: &str,
        judge_model: &str,
    ) -> String {
        Self::sha256(&format!(
            "{}:{}:{}:{}",
            judge_prompt_hash, finding_message, golden_comment, judge_model
        ))
    }

    /// Look up a cached judge verdict by cache key.
    /// Returns `Some(JudgeVerdict)` on hit, `None` on miss.
    pub fn lookup_judge(&self, cache_key: &str) -> Option<JudgeVerdict> {
        let index = self.index.lock().ok()?;
        let entry = index.entries.get(cache_key)?;
        let verdict_path = self.dir.join(&entry.file_path);
        let content = std::fs::read_to_string(&verdict_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save a judge verdict with its cache key and update the index.
    pub fn save_judge_cached(
        &self,
        cache_key: &str,
        _golden: &str,
        _finding: &str,
        verdict_json: &str,
    ) -> Result<()> {
        let verdict_path = self.dir.join("judge").join(format!("{cache_key}.json"));
        std::fs::write(&verdict_path, verdict_json)?;

        // Update index
        let mut index = self.index.lock().map_err(|e| format!("cache index lock: {e}"))?;
        index.entries.insert(
            cache_key.to_string(),
            CacheEntry {
                file_path: format!("judge/{cache_key}.json"),
                timestamp: Self::now(),
                model: String::new(),
                tokens_used: None,
            },
        );
        index.save(&self.index_path());
        Ok(())
    }

    // ── Context gatherer cache ───────────────────────────────────────────

    /// Compute a content-addressed cache key for a context gatherer LLM call.
    pub fn compute_context_key(
        gatherer_prompt_hash: &str,
        diff_hash: &str,
        repo_state_hash: &str,
        model_name: &str,
    ) -> String {
        Self::sha256(&format!(
            "{}:{}:{}:{}",
            gatherer_prompt_hash, diff_hash, repo_state_hash, model_name
        ))
    }

    /// Look up a cached context gatherer response by cache key.
    pub fn lookup_context(&self, cache_key: &str) -> Option<String> {
        let index = self.index.lock().ok()?;
        let entry = index.entries.get(cache_key)?;
        let response_path = self.dir.join(&entry.file_path);
        std::fs::read_to_string(&response_path).ok()
    }

    /// Save a context gatherer prompt+response with its cache key.
    pub fn save_context_cached(
        &self,
        cache_key: &str,
        prompt: &str,
        response: &str,
    ) -> Result<()> {
        let prompt_path = self.dir.join("context").join(format!("{cache_key}.context_prompt.txt"));
        let response_path = self.dir.join("context").join(format!("{cache_key}.context_response.txt"));

        std::fs::write(&prompt_path, prompt)?;
        std::fs::write(&response_path, response)?;

        let mut index = self.index.lock().map_err(|e| format!("cache index lock: {e}"))?;
        index.entries.insert(
            cache_key.to_string(),
            CacheEntry {
                file_path: format!("context/{cache_key}.context_response.txt"),
                timestamp: Self::now(),
                model: String::new(),
                tokens_used: None,
            },
        );
        index.save(&self.index_path());
        Ok(())
    }

    // ── Legacy methods (for backwards compatibility) ───────────────────────

    /// Save an agent prompt+response pair (legacy, without content-addressed key).
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

    /// Append a judge call to the JSONL file (legacy).
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

    // Content-addressed methods

    fn lookup_agent_by_key(&self, cache_key: &str) -> Option<String> {
        let result = self.lookup_agent(cache_key);
        if result.is_some() {
            tracing::debug!("Cache HIT for agent key={}", &cache_key[..12]);
        }
        result
    }

    fn lookup_judge_by_key(&self, cache_key: &str) -> Option<JudgeVerdict> {
        let result = self.lookup_judge(cache_key);
        if result.is_some() {
            tracing::debug!("Cache HIT for judge key={}", &cache_key[..12]);
        }
        result
    }

    fn save_agent_with_key(&self, cache_key: &str, role: &str, prompt: &str, response: &str) {
        if let Err(e) = self.save_agent_cached(cache_key, role, prompt, response) {
            tracing::warn!("Cache save_agent_cached failed: {e}");
        }
    }

    fn save_judge_with_key(&self, cache_key: &str, golden: &str, finding: &str, verdict_json: &str) {
        if let Err(e) = self.save_judge_cached(cache_key, golden, finding, verdict_json) {
            tracing::warn!("Cache save_judge_cached failed: {e}");
        }
    }

    fn lookup_context_by_key(&self, cache_key: &str) -> Option<String> {
        let result = self.lookup_context(cache_key);
        if result.is_some() {
            tracing::debug!("Cache HIT for context key={}", &cache_key[..12]);
        }
        result
    }

    fn save_context_with_key(&self, cache_key: &str, prompt: &str, response: &str) {
        if let Err(e) = self.save_context_cached(cache_key, prompt, response) {
            tracing::warn!("Cache save_context_cached failed: {e}");
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
    fn test_cache_creates_directories() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(dir.path(), "test-pr").unwrap();
        assert!(cache.dir().exists());
        assert!(cache.dir().join("agents").exists());
        assert!(cache.dir().join("judge").exists());
        assert!(cache.dir().join("context").exists());
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
            .save_judge("golden comment", "finding msg", r#"{"match":true}"#)
            .unwrap();

        let path = cache.dir().join("judge_calls.jsonl");
        assert!(path.exists());

        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("golden comment"));
        assert!(content.contains("finding msg"));
        assert!(content.contains(r#""match":true"#));
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

    // ── Content-addressed cache tests ─────────────────────────────────

    #[test]
    fn test_compute_agent_key_deterministic() {
        let key1 = LlmCache::compute_agent_key("abc", "def", "gpt-4o", "SA", "rules123");
        let key2 = LlmCache::compute_agent_key("abc", "def", "gpt-4o", "SA", "rules123");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_compute_agent_key_different_inputs() {
        let key1 = LlmCache::compute_agent_key("abc", "def", "gpt-4o", "SA", "rules123");
        let key2 = LlmCache::compute_agent_key("abc", "xyz", "gpt-4o", "SA", "rules123");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_agent_cache_hit_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(dir.path(), "test-pr").unwrap();

        let key = LlmCache::compute_agent_key("ph", "dh", "gpt-4o", "SA", "rh");
        // Should miss initially
        assert!(cache.lookup_agent(&key).is_none());

        // Save and re-check
        cache.save_agent_cached(&key, "SA", "prompt", "response").unwrap();
        assert_eq!(cache.lookup_agent(&key).unwrap(), "response");
    }

    #[test]
    fn test_judge_cache_hit_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(dir.path(), "test-pr").unwrap();

        let key = LlmCache::compute_judge_key("jph", "finding", "golden", "judge-model");
        assert!(cache.lookup_judge(&key).is_none());

        let verdict = r#"{"reasoning":"test","match":true,"confidence":0.95}"#;
        cache.save_judge_cached(&key, "golden", "finding", verdict).unwrap();

        let cached: JudgeVerdict = cache.lookup_judge(&key).unwrap();
        assert!(cached.match_);
        assert!((cached.confidence - 0.95).abs() < 1e-6);
    }

    #[test]
    fn test_context_cache_hit_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(dir.path(), "test-pr").unwrap();

        let key = LlmCache::compute_context_key("gph", "dh", "rh", "gpt-4o");
        assert!(cache.lookup_context(&key).is_none());

        cache.save_context_cached(&key, "context prompt", "context response").unwrap();
        assert_eq!(cache.lookup_context(&key).unwrap(), "context response");
    }

    #[test]
    fn test_index_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        // Create cache and save some entries
        {
            let cache = LlmCache::new(&path, "test-pr").unwrap();
            let key = LlmCache::compute_agent_key("ph", "dh", "gpt-4o", "SA", "rh");
            cache.save_agent_cached(&key, "SA", "prompt", "response").unwrap();
        }

        // Re-load and verify index persists
        {
            let cache = LlmCache::new(&path, "test-pr").unwrap();
            let key = LlmCache::compute_agent_key("ph", "dh", "gpt-4o", "SA", "rh");
            assert!(cache.lookup_agent(&key).is_some());
        }
    }

    #[test]
    fn test_cache_key_hex_length() {
        let key = LlmCache::compute_agent_key("abc", "def", "model", "SA", "rules");
        assert_eq!(key.len(), 64); // SHA256 hex is 64 chars
    }
}
