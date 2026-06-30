# Cache CLI Spec

## Commands enum additions

File: `crates/crb-benchmark/src/main.rs`

### `CacheStats` variant
```rust
/// Show cache contents and statistics.
CacheStats {
    /// Cache directory (default: "cache").
    #[arg(long, env = "CACHE_DIR", default_value = "cache")]
    cache_dir: PathBuf,
    /// Output as JSON instead of table.
    #[arg(long, default_value_t = false)]
    json: bool,
}
```

### `CachePrune` variant
```rust
/// Evict cache entries by age, size, or count.
CachePrune {
    /// Cache directory (default: "cache").
    #[arg(long, env = "CACHE_DIR", default_value = "cache")]
    cache_dir: PathBuf,
    /// Maximum age of entries in days (entries older than this are removed).
    #[arg(long)]
    max_age: Option<u64>,
    /// Maximum total cache size in bytes.
    #[arg(long)]
    max_size: Option<u64>,
    /// Maximum number of PR directories to keep (oldest evicted first).
    #[arg(long)]
    max_prs: Option<usize>,
    /// Dry run: show what would be removed without removing.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    /// Output as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}
```

### `CacheScrub` variant
```rust
/// Reconcile index.json with filesystem; find orphans and stale entries.
CacheScrub {
    /// Cache directory (default: "cache").
    #[arg(long, env = "CACHE_DIR", default_value = "cache")]
    cache_dir: PathBuf,
    /// Dry run: show issues without fixing.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    /// Repair found issues (remove orphans, rebuild indices).
    #[arg(long, default_value_t = false)]
    repair: bool,
    /// Output as JSON.
    #[arg(long, default_value_t = false)]
    json: bool,
}
```

### `CacheBackup` variant
```rust
/// Create a timestamped tarball snapshot of the cache.
CacheBackup {
    /// Cache directory (default: "cache").
    #[arg(long, env = "CACHE_DIR", default_value = "cache")]
    cache_dir: PathBuf,
    /// Output path for the backup tarball (default: cache/backup_{timestamp}.tar.gz).
    #[arg(long)]
    output: Option<PathBuf>,
}
```

### `CacheRestore` variant
```rust
/// Restore cache from a backup tarball.
CacheRestore {
    /// Path to the backup tarball to restore.
    backup_file: PathBuf,
    /// Cache directory to restore to (default: "cache").
    #[arg(long, env = "CACHE_DIR", default_value = "cache")]
    cache_dir: PathBuf,
}
```

### `CacheRebuild` variant
```rust
/// Rebuild cache indices (experimental — for prompt hash migration).
CacheRebuild {
    /// Cache directory (default: "cache").
    #[arg(long, env = "CACHE_DIR", default_value = "cache")]
    cache_dir: PathBuf,
    /// Dry run: show what would change without modifying.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}
```

### `Clean` extended with `--outputs`
```rust
/// Remove worktrees and optionally diffs / output directories.
Clean {
    #[arg(long, env = "BENCHMARK_DIR", default_value = "benchmark")]
    benchmark_dir: PathBuf,
    /// Also remove diffs directory.
    #[arg(long, default_value_t = false)]
    all: bool,
    /// Also remove output/ directories.
    #[arg(long, default_value_t = false)]
    outputs: bool,
    /// Dry run: show what would be removed.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}
```

### `Run` extended with `--auto-backup`
```rust
// Add to Run struct:
/// Automatically create a cache backup before running.
#[arg(long, default_value_t = false)]
auto_backup: bool,
```

## Handler functions

Each command gets a synchronous handler (cache operations are filesystem-only, no async needed):

```rust
fn run_cache_stats(cache_dir: &PathBuf, json: bool) -> Result<()> { ... }
fn run_cache_prune(cache_dir: &PathBuf, max_age: Option<u64>, max_size: Option<u64>, max_prs: Option<usize>, dry_run: bool, json: bool) -> Result<()> { ... }
fn run_cache_scrub(cache_dir: &PathBuf, dry_run: bool, repair: bool, json: bool) -> Result<()> { ... }
fn run_cache_backup(cache_dir: &PathBuf, output: Option<PathBuf>) -> Result<()> { ... }
fn run_cache_restore(backup_file: &PathBuf, cache_dir: &PathBuf) -> Result<()> { ... }
fn run_cache_rebuild(cache_dir: &PathBuf, dry_run: bool) -> Result<()> { ... }
```

Extended:
```rust
fn run_clean(benchmark_dir: &PathBuf, all: bool, outputs: bool, dry_run: bool) -> Result<()> { ... }
```
