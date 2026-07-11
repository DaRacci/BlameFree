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
use std::time::{Duration, SystemTime};

use crb_consensus::CacheBackend;
use crb_judge::JudgeVerdict;
use rig_core::completion::Usage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A single entry in the run history log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunHistoryEntry {
    pub run_id: String,
    pub timestamp: String,

    pub model: String,
    pub judge_model: String,
    pub total_prs: usize,
    pub duration_secs: f64,
    pub total_cost_usd: f64,
    pub total_tokens: usize,

    pub agent_cache_hit_rate: f64,
    pub judge_cache_hit_rate: f64,
}

/// A single entry in the cache index.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// Relative path from the PR cache directory to the cached file.
    file_path: String,

    /// Unix epoch timestamp with nanosecond precision (seconds.nanoseconds).
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
    /// Maps cache_key -> entry metadata.
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
        if let Err(e) = std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
        {
            tracing::warn!("Failed to write cache index: {e}");
        }
    }
}

/// Statistics for a single PR's cache usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrCacheStats {
    /// The PR key identifying this cache subdirectory.
    pub pr_key: String,
    /// Number of entries in the cache index.
    pub entry_count: usize,
    /// Total byte size of the PR cache directory on disk.
    pub total_size_bytes: u64,
    /// Timestamp of the oldest cached entry, if any.
    pub oldest_entry: Option<String>,
    /// Timestamp of the newest cached entry, if any.
    pub newest_entry: Option<String>,
}

/// Aggregate statistics across all PRs in a cache directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCacheStats {
    /// Number of PR directories found.
    pub pr_count: usize,

    /// Total entries across all PR indices.
    pub total_entries: usize,

    /// Total byte size across all PR cache directories.
    pub total_size_bytes: u64,

    /// Per-PR breakdown of cache stats.
    pub per_pr: Vec<PrCacheStats>,
}

/// Result of a cache prune operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneResult {
    /// Number of PR directories completely removed.
    pub prs_removed: usize,

    /// Total entries removed across all PRs.
    pub entries_removed: usize,

    /// Total bytes freed by removing entries/files.
    pub bytes_freed: u64,

    /// Number of PR directories kept after pruning.
    pub prs_kept: usize,
}

/// Result of a cache scrub operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrubResult {
    /// Number of PR directories scanned.
    pub pr_dirs_scanned: usize,

    /// Stale entries found (files missing from disk).
    pub stale_entries_found: usize,

    /// Orphan files found (on disk but not in index).
    pub orphan_files_found: usize,

    /// Corrupted index files found.
    pub corrupted_indices_found: usize,

    /// Indices rebuilt from filesystem scan.
    pub indices_rebuilt: usize,

    /// Stale entries that were removed (if repair mode).
    pub stale_entries_removed: usize,

    /// Orphan files that were removed (if repair mode).
    pub orphan_files_removed: usize,
}

/// Parse a `seconds.nanoseconds` timestamp (as produced by [`LlmCache::now()`])
/// into a [`SystemTime`].  The format is:
/// `1782796789.721282172`
pub fn parse_timestamp(ts: &str) -> Option<SystemTime> {
    let ts = ts.trim();
    let (secs_str, nanos_str) = ts.split_once('.')?;
    let secs: u64 = secs_str.parse().ok()?;
    let nanos: u32 = nanos_str.chars().take(9).collect::<String>().parse().ok()?;
    Some(std::time::UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_nanos(nanos as u64))
}

/// Recursively compute the total size in bytes of all files under `path`.
pub fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_dir() {
                total += dir_size(&entry.path())?;
            } else if ty.is_file() {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

/// Iterate over PR subdirectories in a cache base directory, skipping
/// hidden/underscore-prefixed entries and non-directories.
///
/// Returns `(pr_key, pr_dir)` pairs.
pub fn collect_pr_dirs(base_dir: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut dirs = Vec::new();
    let read_dir = std::fs::read_dir(base_dir)?;
    for entry in read_dir {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('_') || name_str.starts_with('.') {
            continue;
        }
        if !entry.file_type()?.is_dir() {
            continue;
        }
        dirs.push((name_str.to_string(), entry.path()));
    }
    Ok(dirs)
}

/// Load a cache index from a PR directory. Returns `None` if the index is
/// missing or unparseable.
fn load_cache_index(pr_dir: &Path) -> Option<(CacheIndex, String)> {
    let index_path = pr_dir.join(crate::paths::INDEX_FILE);
    let content = std::fs::read_to_string(&index_path).ok()?;
    let idx: CacheIndex = serde_json::from_str(&content).ok()?;
    Some((idx, content))
}

/// Iterate all files in the standard cache subdirectories (agents/, judge/, context/)
/// under a PR directory, yielding `(subdir_name, filename, rel_path)` for each file found.
fn iter_subdir_files(pr_dir: &Path) -> Vec<(String, String, String)> {
    let mut files = Vec::new();
    let subdirs = ["agents", "judge", "context"];
    for subdir_name in &subdirs {
        let subdir = pr_dir.join(subdir_name);
        if !subdir.exists() {
            continue;
        }
        if let Ok(read_subdir) = std::fs::read_dir(&subdir) {
            for file_entry in read_subdir {
                let file_entry = match file_entry {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                if !file_entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let fname = file_entry.file_name().to_string_lossy().to_string();
                let rel_path = format!("{}/{}", subdir_name, fname);
                files.push((subdir_name.to_string(), fname, rel_path));
            }
        }
    }
    files
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
        std::fs::create_dir_all(dir.join(crate::paths::AGENTS_DIR))?;
        std::fs::create_dir_all(dir.join(crate::paths::JUDGE_DIR))?;
        std::fs::create_dir_all(dir.join(crate::paths::CONTEXT_DIR))?;

        // Load existing index if any
        let index_path = dir.join(crate::paths::INDEX_FILE);
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

    /// Return the number of entries currently in the cache index.
    pub fn entry_count(&self) -> usize {
        self.index.lock().map(|ix| ix.entries.len()).unwrap_or(0)
    }

    /// Compute a SHA256 hex digest of the input string.
    pub fn sha256(input: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join(crate::paths::INDEX_FILE)
    }

    /// Generate a timestamp string for the current time.
    fn now() -> String {
        let dur = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}.{:09}", dur.as_secs(), dur.subsec_nanos())
    }

    /// Save agent reasoning/thinking text to cache.
    pub fn save_agent_reasoning(&self, cache_key: &str, role: &str, reasoning: &str) -> Result<()> {
        let reasoning_path = self
            .dir
            .join("agents")
            .join(format!("{cache_key}.agent_{role}_reasoning.txt"));
        std::fs::write(&reasoning_path, reasoning)?;
        Ok(())
    }

    // ── Legacy methods (for backwards compatibility) ───────────────────────

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
        self.lookup_entry(cache_key)
    }

    /// Look up a cached agent response by cache key, also returning usage if available.
    /// Returns `Some((response_text, Option<usage>))` on hit.
    pub fn lookup_agent_with_usage(&self, cache_key: &str) -> Option<(String, Option<Usage>)> {
        let index = self.index.lock().ok()?;
        let entry = index.entries.get(cache_key)?;
        let response_path = self.dir.join(&entry.file_path);
        let response = std::fs::read_to_string(&response_path).ok()?;

        // Try to read usage from the corresponding usage JSON file
        // Usage files follow the pattern: agents/{cache_key}.agent_{role}_usage.json
        // We extract the role from the entry file_path: "agents/{key}.agent_{role}_response.txt"
        let usage = entry.file_path.rsplit_once('.').and_then(|(stem, _ext)| {
            // stem looks like "agents/{key}.agent_{role}_response"
            // We need to replace "_response" with "_usage.json"
            let usage_stem = stem.strip_suffix("_response")?;
            let usage_path = self.dir.join(format!("{usage_stem}_usage.json"));
            std::fs::read_to_string(usage_path)
                .ok()
                .and_then(|content| serde_json::from_str::<Usage>(&content).ok())
        });

        Some((response, usage))
    }

    /// Save an agent prompt+response with its cache key and update the index.
    /// Also saves usage data as a separate JSON file if provided.
    pub fn save_agent_cached(
        &self,
        cache_key: &str,
        role: &str,
        prompt: &str,
        response: &str,
    ) -> Result<()> {
        self.save_agent_cached_with_usage(cache_key, role, prompt, response, None)
    }

    /// Save an agent prompt+response with its cache key, including API usage.
    pub fn save_agent_cached_with_usage(
        &self,
        cache_key: &str,
        role: &str,
        prompt: &str,
        response: &str,
        usage: Option<&Usage>,
    ) -> Result<()> {
        // Write prompt and response files
        let prompt_path = self
            .dir
            .join("agents")
            .join(format!("{cache_key}.agent_{role}_prompt.txt"));
        let response_path = self
            .dir
            .join("agents")
            .join(format!("{cache_key}.agent_{role}_response.txt"));

        std::fs::write(&prompt_path, prompt)?;
        std::fs::write(&response_path, response)?;

        // Write usage data as JSON if provided
        if let Some(usage) = usage {
            let usage_path = self
                .dir
                .join("agents")
                .join(format!("{cache_key}.agent_{role}_usage.json"));
            if let Err(e) = std::fs::write(
                &usage_path,
                serde_json::to_string(usage).unwrap_or_default(),
            ) {
                tracing::warn!("Failed to write agent usage cache: {e}");
            }
        }

        // Update index
        self.update_index(
            cache_key,
            format!("agents/{cache_key}.agent_{role}_response.txt"),
        )?;
        Ok(())
    }

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
        self.update_index(cache_key, format!("judge/{cache_key}.json"))?;
        Ok(())
    }

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
        self.lookup_entry(cache_key)
    }

    /// Save a context gatherer prompt+response with its cache key.
    pub fn save_context_cached(&self, cache_key: &str, prompt: &str, response: &str) -> Result<()> {
        let prompt_path = self
            .dir
            .join("context")
            .join(format!("{cache_key}.context_prompt.txt"));
        let response_path = self
            .dir
            .join("context")
            .join(format!("{cache_key}.context_response.txt"));

        std::fs::write(&prompt_path, prompt)?;
        std::fs::write(&response_path, response)?;

        self.update_index(
            cache_key,
            format!("context/{cache_key}.context_response.txt"),
        )?;
        Ok(())
    }

    /// Insert or update a cache entry in the index and persist immediately.
    fn update_index(&self, cache_key: &str, file_path: String) -> Result<()> {
        let mut index = self
            .index
            .lock()
            .map_err(|e| format!("cache index lock: {e}"))?;
        index.entries.insert(
            cache_key.to_string(),
            CacheEntry {
                file_path,
                timestamp: Self::now(),
                model: String::new(),
                tokens_used: None,
            },
        );
        index.save(&self.index_path());
        Ok(())
    }

    /// Look up a cached entry by cache key and return the file content as a string.
    fn lookup_entry(&self, cache_key: &str) -> Option<String> {
        let index = self.index.lock().ok()?;
        let entry = index.entries.get(cache_key)?;
        let response_path = self.dir.join(&entry.file_path);
        std::fs::read_to_string(&response_path).ok()
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
    pub fn save_judge(&self, golden: &str, finding: &str, verdict_json: &str) -> Result<()> {
        let path = self.dir.join("judge_calls.jsonl");
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let timestamp = {
            let dur = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            format!("{}.{:09}", dur.as_secs(), dur.subsec_nanos())
        };
        let entry = serde_json::json!({
            "timestamp": timestamp,
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
        std::fs::create_dir_all(&self.dir)?;
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

    /// Gather statistics across all PR directories under `base_dir`.
    ///
    /// Walks all subdirectories, reads each `index.json`, counts entries,
    /// computes on-disk sizes, and tracks oldest/newest timestamps.
    /// Directories or files whose name starts with `_` or `.` are skipped.
    pub fn stats(base_dir: &Path) -> Result<GlobalCacheStats> {
        let mut per_pr = Vec::new();
        let mut total_entries = 0usize;
        let mut total_size_bytes = 0u64;
        let mut pr_count = 0usize;

        let dirs = collect_pr_dirs(base_dir)
            .map_err(|e| format!("cannot read cache dir {}: {}", base_dir.display(), e))?;

        for (pr_key, pr_dir) in &dirs {
            let index_path = pr_dir.join(crate::paths::INDEX_FILE);

            let (entry_count, oldest, newest) = match std::fs::read_to_string(&index_path) {
                Ok(content) => {
                    if let Ok(idx) = serde_json::from_str::<CacheIndex>(&content) {
                        let count = idx.entries.len();
                        let oldest = idx.entries.values().map(|e| &e.timestamp).min().cloned();
                        let newest = idx.entries.values().map(|e| &e.timestamp).max().cloned();
                        (count, oldest, newest)
                    } else {
                        (0, None, None)
                    }
                }
                Err(_) => (0, None, None),
            };

            let size = dir_size(&pr_dir).unwrap_or(0);

            total_entries += entry_count;
            total_size_bytes += size;
            pr_count += 1;

            per_pr.push(PrCacheStats {
                pr_key: pr_key.clone(),
                entry_count,
                total_size_bytes: size,
                oldest_entry: oldest,
                newest_entry: newest,
            });
        }

        Ok(GlobalCacheStats {
            pr_count,
            total_entries,
            total_size_bytes,
            per_pr,
        })
    }

    /// Prune old cache entries under `base_dir` according to the given filters.
    ///
    /// Supported filters:
    /// - `max_age_days`: remove entries whose timestamp is older than N days.
    /// - `max_size_bytes`: evict the oldest entries until the PR directory
    ///   is below this size.
    /// - `max_prs`: keep only the N newest PR directories (by newest entry).
    ///
    /// When `dry_run` is true, only report what would happen without actually
    /// removing anything.
    #[allow(clippy::too_many_arguments)]
    pub fn prune(
        base_dir: &Path,
        max_age_days: Option<u64>,
        max_size_bytes: Option<u64>,
        max_prs: Option<usize>,
        dry_run: bool,
    ) -> Result<PruneResult> {
        let mut result = PruneResult {
            prs_removed: 0,
            entries_removed: 0,
            bytes_freed: 0,
            prs_kept: 0,
        };
        let index_path = base_dir.join(crate::paths::INDEX_FILE);

        // Collect all PR directories
        let mut pr_dirs: Vec<(String, PathBuf, Option<String>)> = Vec::new();
        let dirs = collect_pr_dirs(base_dir)
            .map_err(|e| format!("cannot read cache dir {}: {}", base_dir.display(), e))?;

        for (pr_key, pr_dir) in &dirs {
            let index_path = pr_dir.join(crate::paths::INDEX_FILE);

            // Find the newest entry timestamp for this PR
            let newest = std::fs::read_to_string(&index_path)
                .ok()
                .and_then(|content| {
                    serde_json::from_str::<CacheIndex>(&content)
                        .ok()
                        .and_then(|idx| idx.entries.values().map(|e| &e.timestamp).max().cloned())
                });

            pr_dirs.push((pr_key.clone(), pr_dir.clone(), newest));
        }

        if let Some(max) = max_prs {
            if pr_dirs.len() > max {
                // Sort by newest entry (descending), keep first `max`
                pr_dirs.sort_by(|a, b| {
                    b.2.as_deref()
                        .unwrap_or("")
                        .cmp(&a.2.as_deref().unwrap_or(""))
                });
                let to_remove: Vec<_> = pr_dirs.drain(max..).collect();
                for (_pr_key, pr_dir, _newest) in &to_remove {
                    let size = dir_size(pr_dir).unwrap_or(0);
                    result.prs_removed += 1;
                    result.bytes_freed += size;
                    if !dry_run {
                        let _ = std::fs::remove_dir_all(pr_dir);
                    }
                }
                result.prs_kept = pr_dirs.len();
            } else {
                result.prs_kept = pr_dirs.len();
            }
        } else {
            result.prs_kept = pr_dirs.len();
        }

        if let Some(days) = max_age_days {
            let cutoff = Duration::from_secs(days * 86400);
            let now = SystemTime::now();

            for (_pr_key, pr_dir, _newest) in &pr_dirs {
                let (mut idx, content) = match load_cache_index(pr_dir) {
                    Some(result) => result,
                    None => continue,
                };

                let before = idx.entries.len();
                let mut removed_bytes = 0u64;

                idx.entries.retain(|_key, entry| {
                    let keep = match parse_timestamp(&entry.timestamp) {
                        Some(t) => match now.duration_since(t) {
                            Ok(age) => age <= cutoff,
                            Err(_) => true,
                        },
                        None => true,
                    };
                    if !keep {
                        // Track the file size
                        let file_path = pr_dir.join(&entry.file_path);
                        if let Ok(meta) = std::fs::metadata(&file_path) {
                            removed_bytes += meta.len();
                        }
                    }
                    keep
                });

                let after = idx.entries.len();
                if before > after {
                    let entry_count = before - after;
                    result.entries_removed += entry_count;
                    result.bytes_freed += removed_bytes;
                    if !dry_run {
                        idx.save(&index_path);
                        // Remove files for pruned entries
                        let current_keys: std::collections::HashSet<String> =
                            idx.entries.keys().cloned().collect();
                        // Only remove files for entries that were actually removed
                        if let Ok(old_content) = serde_json::from_str::<CacheIndex>(&content) {
                            for (old_key, old_entry) in &old_content.entries {
                                if !current_keys.contains(old_key) {
                                    let file_path = pr_dir.join(&old_entry.file_path);
                                    let _ = std::fs::remove_file(&file_path);
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(max_size) = max_size_bytes {
            for (_pr_key, pr_dir, _newest) in &pr_dirs {
                let current_size = dir_size(pr_dir).unwrap_or(0);
                if current_size <= max_size {
                    continue;
                }
                let (mut idx, _content) = match load_cache_index(pr_dir) {
                    Some(result) => result,
                    None => continue,
                };

                // Collect entries sorted by timestamp (oldest first)
                let mut entries: Vec<(String, CacheEntry)> = idx.entries.drain().collect();
                entries.sort_by(|a, b| a.1.timestamp.cmp(&b.1.timestamp));

                let mut running_size = current_size;
                let mut kept = Vec::new();
                let mut removed_count = 0usize;
                let mut removed_bytes = 0u64;

                for (key, entry) in entries {
                    if running_size > max_size {
                        let file_path = pr_dir.join(&entry.file_path);
                        if let Ok(meta) = std::fs::metadata(&file_path) {
                            running_size = running_size.saturating_sub(meta.len());
                            removed_bytes += meta.len();
                        } else {
                            // File already gone, just reduce conceptual size
                            running_size = running_size.saturating_sub(1024);
                        }
                        removed_count += 1;
                        if !dry_run {
                            let file_path = pr_dir.join(&entry.file_path);
                            let _ = std::fs::remove_file(&file_path);
                        }
                    } else {
                        kept.push((key, entry));
                    }
                }

                if removed_count > 0 {
                    result.entries_removed += removed_count;
                    result.bytes_freed += removed_bytes;
                    if !dry_run {
                        idx.entries = kept.into_iter().collect();
                        idx.save(&index_path);
                    }
                }
            }
        }

        // Remove empty PR directories after pruning
        if !dry_run {
            for (_pr_key, pr_dir, _newest) in &pr_dirs {
                if pr_dir.exists() && pr_dir.is_dir() {
                    let is_empty = std::fs::read_dir(pr_dir)
                        .map(|mut r| r.next().is_none())
                        .unwrap_or(false);
                    if is_empty {
                        let _ = std::fs::remove_dir(pr_dir);
                        result.prs_removed += 1;
                    }
                }
            }
        }

        Ok(result)
    }

    /// Scrub the cache for consistency issues.
    ///
    /// For each PR directory:
    /// - Verifies every file referenced in `index.json` actually exists on disk
    ///   (stale entries).
    /// - Scans the `agents/`, `judge/`, `context/` subdirectories for files not
    ///   referenced in the index (orphans).
    /// - If `index.json` is missing or corrupt, scans the filesystem to rebuild it.
    ///
    /// When `repair` is true, removes stale entries, removes orphan files, and
    /// writes corrected index files.
    pub fn scrub(base_dir: &Path, dry_run: bool, repair: bool) -> Result<ScrubResult> {
        let mut result = ScrubResult {
            pr_dirs_scanned: 0,
            stale_entries_found: 0,
            orphan_files_found: 0,
            corrupted_indices_found: 0,
            indices_rebuilt: 0,
            stale_entries_removed: 0,
            orphan_files_removed: 0,
        };

        let read_dir = match std::fs::read_dir(base_dir) {
            Ok(d) => d,
            Err(e) => {
                return Err(format!("cannot read cache dir {}: {}", base_dir.display(), e).into())
            }
        };

        for entry in read_dir {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('_') || name_str.starts_with('.') {
                continue;
            }
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let pr_dir = entry.path();
            result.pr_dirs_scanned += 1;

            let index_path = pr_dir.join(crate::paths::INDEX_FILE);
            let index_exists = index_path.exists();

            // Try to load the index
            let mut idx: CacheIndex = if index_exists {
                match std::fs::read_to_string(&index_path) {
                    Ok(content) => match serde_json::from_str(&content) {
                        Ok(i) => i,
                        Err(_) => {
                            result.corrupted_indices_found += 1;
                            // Corrupted - will rebuild
                            CacheIndex::new()
                        }
                    },
                    Err(_) => CacheIndex::new(),
                }
            } else {
                CacheIndex::new()
            };

            let mut needs_rebuild = false;
            if !index_exists || result.corrupted_indices_found > 0 {
                needs_rebuild = true;
            }

            // ── Check for stale entries (file referenced in index but missing on disk) ──
            let mut stale_keys = Vec::new();
            for (key, entry_meta) in &idx.entries {
                let file_path = pr_dir.join(&entry_meta.file_path);
                if !file_path.exists() {
                    stale_keys.push(key.clone());
                }
            }

            // Remove stale entries from index
            for key in &stale_keys {
                idx.entries.remove(key);
                result.stale_entries_found += 1;
                if repair && !dry_run {
                    result.stale_entries_removed += 1;
                }
            }

            // ── Check for orphan files (on disk but not in index) ──
            let mut orphan_files = Vec::new();
            for (_subdir_name, fname, rel_path) in iter_subdir_files(pr_dir.as_path()) {
                // Extract cache key from filename (everything before first '.')
                // Companion files (prompts, usage) share the same key prefix
                let file_key = fname.split('.').next().unwrap_or(&fname).to_string();
                // Check if this file is referenced in any entry, or if its
                // cache key matches any indexed entry (companion file)
                let is_indexed = idx.entries.contains_key(&file_key)
                    || idx.entries.values().any(|e| e.file_path == rel_path);
                if !is_indexed {
                    orphan_files.push(rel_path);
                }
            }

            result.orphan_files_found += orphan_files.len();

            // Remove orphan files
            if repair && !dry_run {
                for rel_path in &orphan_files {
                    let file_path = pr_dir.join(rel_path);
                    if std::fs::remove_file(&file_path).is_ok() {
                        result.orphan_files_removed += 1;
                    }
                }
            }

            // ── Rebuild index from filesystem if needed ──
            if needs_rebuild && !dry_run {
                let mut new_idx = CacheIndex::new();
                for (_subdir_name, fname, rel_path) in iter_subdir_files(pr_dir.as_path()) {
                    // Extract a cache key from the filename
                    // Filenames look like: {cache_key}.agent_SA_response.txt
                    // or {cache_key}.json, or {cache_key}.context_response.txt
                    let cache_key = fname.split('.').next().unwrap_or(&fname).to_string();

                    if !new_idx.entries.contains_key(&cache_key) {
                        new_idx.entries.insert(
                            cache_key,
                            CacheEntry {
                                file_path: rel_path,
                                timestamp: LlmCache::now(),
                                model: String::new(),
                                tokens_used: None,
                            },
                        );
                    }
                }
                idx = new_idx;
                result.indices_rebuilt += 1;
            }

            // Write the (potentially corrected) index
            if repair && !dry_run {
                idx.save(&index_path);
            }
        }

        Ok(result)
    }

    /// Create a compressed tarball backup of the cache directory.
    pub fn backup(base_dir: &Path, output_path: &Path) -> Result<()> {
        let output_str = output_path
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 output path: {}", output_path.display()))?;

        let status = std::process::Command::new("tar")
            .args(["-czf", output_str, "."])
            .current_dir(base_dir)
            .status()
            .map_err(|e| format!("failed to execute tar: {e}"))?;

        if !status.success() {
            return Err(format!("tar backup failed with exit code: {:?}", status.code()).into());
        }
        Ok(())
    }

    /// Restore a cache directory from a backup tarball.
    ///
    /// Creates `base_dir` if it does not exist.
    pub fn restore(base_dir: &Path, backup_file: &Path) -> Result<()> {
        std::fs::create_dir_all(base_dir)?;

        let backup_str = backup_file
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 backup path: {}", backup_file.display()))?;

        let status = std::process::Command::new("tar")
            .args(["-xzf", backup_str])
            .current_dir(base_dir)
            .status()
            .map_err(|e| format!("failed to execute tar: {e}"))?;

        if !status.success() {
            return Err(format!("tar restore failed with exit code: {:?}", status.code()).into());
        }
        Ok(())
    }

    /// Rebuild cache keys by re-computing the SHA256 hash of entry metadata.
    ///
    /// For each entry, re-hashes the file_path + model + tokens that were stored
    /// in the index.  If the new key differs from the current key, the file is
    /// renamed and the index updated (unless `dry_run`).
    ///
    /// This is experimental and primarily useful for testing or migrating
    /// key schemas.
    pub fn rebuild(base_dir: &Path, dry_run: bool) -> Result<()> {
        let dirs = collect_pr_dirs(base_dir)
            .map_err(|e| format!("cannot read cache dir {}: {}", base_dir.display(), e))?;

        for (_pr_key, pr_dir) in &dirs {
            let (mut idx, _content) = match load_cache_index(pr_dir) {
                Some(result) => result,
                None => continue,
            };
            let index_path = pr_dir.join(crate::paths::INDEX_FILE);

            let mut new_entries = HashMap::new();
            for (old_key, entry_meta) in &idx.entries {
                // Re-hash the entry metadata to create a new key
                let meta_str = format!(
                    "{}:{}:{}:{:?}",
                    entry_meta.file_path,
                    entry_meta.timestamp,
                    entry_meta.model,
                    entry_meta.tokens_used,
                );
                let new_key = LlmCache::sha256(&meta_str);

                if new_key != *old_key {
                    tracing::info!(
                        "rebuild: key changed for {}: {} -> {}",
                        entry_meta.file_path,
                        &old_key[..12],
                        &new_key[..12],
                    );

                    if !dry_run {
                        // Rename the file on disk to use the new key
                        let old_path = pr_dir.join(&entry_meta.file_path);
                        if old_path.exists() {
                            // Build new file path by replacing the old key in the path
                            let new_file_path = entry_meta.file_path.replace(old_key, &new_key);
                            let new_path = pr_dir.join(&new_file_path);

                            // Ensure parent directory exists
                            if let Some(parent) = new_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }

                            if let Err(e) = std::fs::rename(&old_path, &new_path) {
                                tracing::warn!(
                                    "rebuild: failed to rename {} -> {}: {e}",
                                    old_path.display(),
                                    new_path.display(),
                                );
                            }

                            new_entries.insert(
                                new_key,
                                CacheEntry {
                                    file_path: new_file_path,
                                    timestamp: entry_meta.timestamp.clone(),
                                    model: entry_meta.model.clone(),
                                    tokens_used: entry_meta.tokens_used,
                                },
                            );
                        } else {
                            // File is missing, drop the entry
                            tracing::warn!(
                                "rebuild: file missing for key {}: {}",
                                &old_key[..12],
                                old_path.display(),
                            );
                        }
                    }
                } else {
                    new_entries.insert(old_key.clone(), entry_meta.clone());
                }
            }

            if !dry_run {
                idx.entries = new_entries;
                idx.save(&index_path);
            }
        }

        Ok(())
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

    fn lookup_agent_by_key_with_usage(&self, cache_key: &str) -> Option<(String, Option<Usage>)> {
        let result = self.lookup_agent_with_usage(cache_key);
        if result.is_some() {
            tracing::debug!("Cache HIT for agent key={} (with usage)", &cache_key[..12]);
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

    fn save_agent_with_key_and_usage(
        &self,
        cache_key: &str,
        role: &str,
        prompt: &str,
        response: &str,
        usage: &Usage,
    ) {
        if let Err(e) =
            self.save_agent_cached_with_usage(cache_key, role, prompt, response, Some(usage))
        {
            tracing::warn!("Cache save_agent_with_key_and_usage failed: {e}");
        }
    }

    fn save_agent_reasoning_with_key(&self, cache_key: &str, role: &str, reasoning: &str) {
        if let Err(e) = self.save_agent_reasoning(cache_key, role, reasoning) {
            tracing::warn!("Cache save_agent_reasoning_with_key failed: {e}");
        }
    }

    fn save_judge_with_key(
        &self,
        cache_key: &str,
        golden: &str,
        finding: &str,
        verdict_json: &str,
    ) {
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
        cache
            .save_agent_cached(&key, "SA", "prompt", "response")
            .unwrap();
        assert_eq!(cache.lookup_agent(&key).unwrap(), "response");
    }

    #[test]
    fn test_judge_cache_hit_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(dir.path(), "test-pr").unwrap();

        let key = LlmCache::compute_judge_key("jph", "finding", "golden", "judge-model");
        assert!(cache.lookup_judge(&key).is_none());

        let verdict = r#"{"reasoning":"test","match":true,"confidence":0.95}"#;
        cache
            .save_judge_cached(&key, "golden", "finding", verdict)
            .unwrap();

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

        cache
            .save_context_cached(&key, "context prompt", "context response")
            .unwrap();
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
            cache
                .save_agent_cached(&key, "SA", "prompt", "response")
                .unwrap();
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

    // ── Cache management tests ─────────────────────────────────────────

    #[test]
    fn test_parse_timestamp() {
        // Format produced by LlmCache::now()
        let ts = LlmCache::now();
        let parsed = parse_timestamp(&ts);
        assert!(parsed.is_some(), "should parse '{}'", ts);

        // Verify the format is seconds.nanoseconds
        assert!(ts.contains('.'), "timestamp should contain '.'");
        let secs: u64 = ts.split('.').next().unwrap().parse().unwrap();
        assert!(secs > 0, "seconds should be > 0 (epoch-based)");

        // Invalid format
        assert!(parse_timestamp("not-a-timestamp").is_none());
        assert!(parse_timestamp("").is_none());
        assert!(parse_timestamp("no-dot").is_none());
        assert!(parse_timestamp("123.bad").is_none());
    }

    #[test]
    fn test_dir_size() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("b.txt"), "world").unwrap();
        let size = dir_size(dir.path()).unwrap();
        assert!(size >= 10); // at least 10 bytes for the two files

        // Empty dir
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(dir_size(empty.path()).unwrap(), 0);
    }

    #[test]
    fn test_cache_stats_basic() {
        let base = tempfile::tempdir().unwrap();

        // Create two PR caches with entries
        {
            let cache1 = LlmCache::new(base.path(), "pr-1").unwrap();
            let key1 = LlmCache::compute_agent_key("a", "b", "m", "SA", "r");
            cache1
                .save_agent_cached(&key1, "SA", "prompt", "resp")
                .unwrap();
            let key2 = LlmCache::compute_judge_key("jph", "f", "g", "jm");
            cache1
                .save_judge_cached(&key2, "g", "f", r#"{"match":true}"#)
                .unwrap();
        }
        {
            let cache2 = LlmCache::new(base.path(), "pr-2").unwrap();
            let key3 = LlmCache::compute_context_key("gph", "dh", "rh", "m");
            cache2
                .save_context_cached(&key3, "ctx", "ctx resp")
                .unwrap();
        }

        let stats = LlmCache::stats(base.path()).unwrap();
        assert_eq!(stats.pr_count, 2);
        assert_eq!(stats.total_entries, 3);

        // Verify per-PR breakdown
        assert_eq!(stats.per_pr.len(), 2);
        let pr1 = stats.per_pr.iter().find(|p| p.pr_key == "pr-1").unwrap();
        assert_eq!(pr1.entry_count, 2);
        assert!(pr1.total_size_bytes > 0);
        assert!(pr1.oldest_entry.is_some());
        assert!(pr1.newest_entry.is_some());

        let pr2 = stats.per_pr.iter().find(|p| p.pr_key == "pr-2").unwrap();
        assert_eq!(pr2.entry_count, 1);
    }

    #[test]
    fn test_cache_prune_dry_run() {
        let base = tempfile::tempdir().unwrap();

        // Create a PR with some entries
        {
            let cache = LlmCache::new(base.path(), "test-pr").unwrap();
            let key1 = LlmCache::compute_agent_key("a", "b", "m", "SA", "r");
            cache.save_agent_cached(&key1, "SA", "p", "r").unwrap();
            let key2 = LlmCache::compute_agent_key("c", "d", "m", "SA", "r");
            cache.save_agent_cached(&key2, "SA", "p", "r").unwrap();
        }

        // Dry run should report entries but not remove them
        let result = LlmCache::prune(
            base.path(),
            Some(0), // max_age_days = 0: cutoff at now
            None,
            None,
            true, // dry_run
        )
        .unwrap();

        // max_age_days=0 means cutoff = 0 * 86400 = 0 seconds
        // So entries created a few ms ago are older than 0 seconds
        // They WILL be reported as removable
        assert_eq!(result.entries_removed, 2);
        assert!(result.bytes_freed > 0);

        // But files should still exist because dry_run=true
        let index_path = base.path().join("test-pr").join("index.json");
        let content = std::fs::read_to_string(&index_path).unwrap();
        let idx: CacheIndex = serde_json::from_str(&content).unwrap();
        assert_eq!(idx.entries.len(), 2);
    }

    #[test]
    fn test_cache_prune_max_age() {
        let base = tempfile::tempdir().unwrap();

        // Create entries
        let key1;
        let key2;
        {
            let cache = LlmCache::new(base.path(), "test-pr").unwrap();
            key1 = LlmCache::compute_agent_key("a", "b", "m", "SA", "r");
            cache.save_agent_cached(&key1, "SA", "p", "r").unwrap();
            key2 = LlmCache::compute_agent_key("c", "d", "m", "SA", "r");
            cache.save_agent_cached(&key2, "SA", "p", "r").unwrap();
        }

        // Manually set one entry's timestamp to be old
        let index_path = base.path().join("test-pr").join("index.json");
        let content = std::fs::read_to_string(&index_path).unwrap();
        let mut idx: CacheIndex = serde_json::from_str(&content).unwrap();

        // Set first entry timestamp to 100 days ago
        if let Some(entry) = idx.entries.get_mut(&key1) {
            let old_time =
                std::time::SystemTime::now() - std::time::Duration::from_secs(100 * 86400);
            entry.timestamp = {
                let dur = old_time
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                format!("{}.{:09}", dur.as_secs(), dur.subsec_nanos())
            };
        }
        std::fs::write(&index_path, serde_json::to_string_pretty(&idx).unwrap()).unwrap();

        // Now prune with max_age_days=30
        let result = LlmCache::prune(base.path(), Some(30), None, None, false).unwrap();

        assert_eq!(result.entries_removed, 1);
        assert!(result.bytes_freed > 0);

        // Verify only the recent entry remains
        let content = std::fs::read_to_string(&index_path).unwrap();
        let idx: CacheIndex = serde_json::from_str(&content).unwrap();
        assert_eq!(idx.entries.len(), 1);
        assert!(idx.entries.contains_key(&key2));
    }

    /// Helper: create a temporary cache with one agent entry for testing.
    fn setup_test_cache() -> (tempfile::TempDir, String) {
        let base = tempfile::tempdir().unwrap();
        let cache = LlmCache::new(base.path(), "test-pr").unwrap();
        let key = LlmCache::compute_agent_key("a", "b", "m", "SA", "r");
        cache.save_agent_cached(&key, "SA", "p", "r").unwrap();
        drop(cache);
        (base, key)
    }

    #[test]
    fn test_cache_scrub_orphans() {
        let (base, _key) = setup_test_cache();

        // Inject an orphan file
        let orphan_path = base
            .path()
            .join("test-pr")
            .join("agents")
            .join("orphan_file.txt");
        std::fs::write(&orphan_path, "orphan data").unwrap();

        // Dry-run scrub should detect the orphan
        let result = LlmCache::scrub(base.path(), true, false).unwrap();
        assert_eq!(result.pr_dirs_scanned, 1);
        assert_eq!(result.orphan_files_found, 1);
        assert_eq!(result.stale_entries_found, 0);

        // Orphan should still exist (dry run)
        assert!(orphan_path.exists());
    }

    #[test]
    fn test_cache_scrub_repair() {
        let (base, key) = setup_test_cache();

        // Inject an orphan file
        let orphan_path = base
            .path()
            .join("test-pr")
            .join("agents")
            .join("orphan.txt");
        std::fs::write(&orphan_path, "orphan").unwrap();

        // Repair scrub should remove the orphan
        let result = LlmCache::scrub(base.path(), false, true).unwrap();
        assert_eq!(result.orphan_files_found, 1);
        assert_eq!(result.orphan_files_removed, 1);

        // Orphan should be gone
        assert!(!orphan_path.exists());

        // Original entry should still be there
        assert!(key.len() == 64);
        let index_path = base.path().join("test-pr").join("index.json");
        let content = std::fs::read_to_string(&index_path).unwrap();
        let idx: CacheIndex = serde_json::from_str(&content).unwrap();
        assert_eq!(idx.entries.len(), 1);
    }

    #[test]
    fn test_cache_backup_restore_roundtrip() {
        let base = tempfile::tempdir().unwrap();
        let backup_dir = tempfile::tempdir().unwrap();
        let restore_dir = tempfile::tempdir().unwrap();

        // Create a PR with entries
        {
            let cache = LlmCache::new(base.path(), "test-pr").unwrap();
            let key1 = LlmCache::compute_agent_key("a", "b", "m", "SA", "r");
            cache
                .save_agent_cached(&key1, "SA", "prompt", "response")
                .unwrap();
            let key2 = LlmCache::compute_judge_key("jph", "f", "g", "jm");
            cache
                .save_judge_cached(&key2, "g", "f", r#"{"match":true}"#)
                .unwrap();
        }

        // Backup
        let backup_path = backup_dir.path().join("cache-backup.tar.gz");
        LlmCache::backup(base.path(), &backup_path).unwrap();
        assert!(backup_path.exists());

        // Restore to a different directory
        LlmCache::restore(restore_dir.path(), &backup_path).unwrap();

        // Verify entries are intact in restored directory
        let stats = LlmCache::stats(restore_dir.path()).unwrap();
        assert_eq!(stats.pr_count, 1);
        assert_eq!(stats.total_entries, 2);

        // Verify individual entries
        let cache = LlmCache::new(restore_dir.path(), "test-pr").unwrap();
        let key1 = LlmCache::compute_agent_key("a", "b", "m", "SA", "r");
        assert_eq!(cache.lookup_agent(&key1).unwrap(), "response");
    }

    #[test]
    fn test_cache_rebuild() {
        let base = tempfile::tempdir().unwrap();

        // Create entries
        let key;
        {
            let cache = LlmCache::new(base.path(), "test-pr").unwrap();
            key = LlmCache::compute_agent_key("a", "b", "m", "SA", "r");
            cache
                .save_agent_cached(&key, "SA", "prompt", "response")
                .unwrap();
        }

        // Dry-run rebuild should not change anything
        LlmCache::rebuild(base.path(), true).unwrap();

        // Verify entry is still accessible with old key after dry run
        let cache = LlmCache::new(base.path(), "test-pr").unwrap();
        assert_eq!(cache.lookup_agent(&key).unwrap(), "response");

        // Non-dry-run rebuild will recompute keys (using entry metadata, which
        // includes timestamp, so the key will differ from the original).
        // After rebuild, the index should still have exactly 1 entry.
        LlmCache::rebuild(base.path(), false).unwrap();

        // Index should have 1 entry after rebuild
        let index_path = base.path().join("test-pr").join("index.json");
        let content = std::fs::read_to_string(&index_path).unwrap();
        let idx: CacheIndex = serde_json::from_str(&content).unwrap();
        assert_eq!(idx.entries.len(), 1);

        // The response file should exist on disk
        let cache = LlmCache::new(base.path(), "test-pr").unwrap();
        assert!(cache.entry_count() > 0);

        // The response file should be findable via the new key in the index
        let new_key = idx.entries.keys().next().unwrap();
        assert_eq!(cache.lookup_agent(new_key).unwrap(), "response");
    }

    #[test]
    fn test_cache_prune_max_prs() {
        let base = tempfile::tempdir().unwrap();

        // Create three PR directories
        {
            let cache = LlmCache::new(base.path(), "pr-oldest").unwrap();
            let key = LlmCache::compute_agent_key("a", "b", "m", "SA", "r");
            cache.save_agent_cached(&key, "SA", "p", "r").unwrap();
        }
        {
            let cache = LlmCache::new(base.path(), "pr-middle").unwrap();
            let key = LlmCache::compute_agent_key("c", "d", "m", "SA", "r");
            cache.save_agent_cached(&key, "SA", "p", "r").unwrap();
        }
        {
            let cache = LlmCache::new(base.path(), "pr-newest").unwrap();
            let key = LlmCache::compute_agent_key("e", "f", "m", "SA", "r");
            cache.save_agent_cached(&key, "SA", "p", "r").unwrap();
        }

        // Prune to keep only 1 PR
        let result = LlmCache::prune(base.path(), None, None, Some(1), false).unwrap();

        assert_eq!(result.prs_removed, 2);
        assert_eq!(result.prs_kept, 1);

        // Only 1 PR should remain
        let stats = LlmCache::stats(base.path()).unwrap();
        assert_eq!(stats.pr_count, 1);
    }
}
