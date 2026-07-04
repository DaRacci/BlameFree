# Cache Core Spec

File: `crates/crb-harness/src/cache.rs`

## New structs

### `PrCacheStats`
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrCacheStats {
    pub pr_key: String,
    pub entry_count: usize,
    pub total_size_bytes: u64,
    pub oldest_entry: Option<String>,
    pub newest_entry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCacheStats {
    pub pr_count: usize,
    pub total_entries: usize,
    pub total_size_bytes: u64,
    pub per_pr: Vec<PrCacheStats>,
}
```

### `PruneResult`
```rust
#[derive(Debug, Clone, Serialize)]
pub struct PruneResult {
    pub prs_removed: usize,
    pub entries_removed: usize,
    pub bytes_freed: u64,
    pub prs_kept: usize,
}
```

### `ScrubResult`
```rust
#[derive(Debug, Clone, Serialize)]
pub struct ScrubResult {
    pub pr_dirs_scanned: usize,
    pub stale_entries_found: usize,
    pub orphan_files_found: usize,
    pub corrupted_indices_found: usize,
    pub indices_rebuilt: usize,
    pub stale_entries_removed: usize,
    pub orphan_files_removed: usize,
}
```

### `RunHistoryEntry`
```rust
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
```

## New LlmCache methods

Place all new methods in `impl LlmCache` block.

### `stats(base_dir: &Path) -> Result<GlobalCacheStats>`
Walk all subdirectories of `base_dir` (skipping `_summary.json`, `_runs.json`, etc.).
For each PR directory:
- Read `index.json`
- Count entries
- Walk files to compute total size
- Track oldest/newest timestamps

### `prune(base_dir: &Path, max_age_days: Option<u64>, max_size_bytes: Option<u64>, max_prs: Option<usize>, dry_run: bool) -> Result<PruneResult>`
Apply filters in order:
1. `--max-prs`: Sort PR dirs by newest entry timestamp, keep newest N, remove rest.
2. `--max-age`: Remove PRs where all entries are older than N days.
3. `--max-size`: If total cache > max_size, remove oldest entries until under limit.
If `dry_run`, compute and report what would be removed without actually removing.
Remove empty PR directories after pruning.

### `scrub(base_dir: &Path, dry_run: bool, repair: bool) -> Result<ScrubResult>`
For each PR directory:
- Read `index.json` -> for each entry, stat the referenced file_path
  - File missing -> stale entry
- Scan all files in agents/, judge/, context/ subdirs
  - File not in index -> orphan
- If `index.json` is missing or corrupt, scan filesystem to rebuild it
- If `repair`: remove stale entries from index, remove orphan files, write corrected index

### `backup(base_dir: &Path, output_path: &Path) -> Result<()>`
Create a timestamped tarball of the cache directory using `std::process::Command` calling `tar`.
Format: `{output_path}/cache_backup_{YYYYMMDD_HHMMSS}.tar.gz`
If output_path is not specified, default to `{cache_dir}/cache_backup_{timestamp}.tar.gz`.

### `restore(base_dir: &Path, backup_file: &Path) -> Result<()>`
Extract backup tarball into the cache directory.
Creates cache_dir if it doesn't exist.
Uses `tar -xzf` via `std::process::Command`.

### `rebuild(base_dir: &Path, dry_run: bool) -> Result<()>`
Iterate all PR directories and their index.json entries.
For each entry, recompute the cache key from the entry metadata.
If the key would change (e.g., due to prompt hash update), re-save the entry with the new key.
If `dry_run`, report what would change without modifying.

## Non-public helpers

```rust
/// Parse ISO-8601-like timestamp string to SystemTime.
fn parse_timestamp(ts: &str) -> Option<std::time::SystemTime>;

/// Compute directory size recursively.
fn dir_size(path: &Path) -> std::io::Result<u64>;
```

## Tests

Each new method gets a tempdir-based test following the existing pattern in cache.rs:
- `test_cache_stats_basic()` — create a cache with entries, verify stats
- `test_cache_prune_dry_run()` — verify dry run reports without removal
- `test_cache_prune_actual()` — verify actual removal
- `test_cache_scrub_orphans()` — inject orphan file, verify detection
- `test_cache_scrub_repair()` — corrupt index, verify rebuild
- `test_cache_backup_restore_roundtrip()` — backup then restore, verify entries intact
