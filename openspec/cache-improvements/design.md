# Cache Improvements — Architecture & Design

## Overview

New cache management capabilities are added to `crb-harness/src/cache.rs` (core logic) and exposed via `crb-benchmark/src/main.rs` (CLI subcommands). The `_summary.json` format is extended with `_runs.json` (append-only run history). No backward compatibility is maintained for the old summary format.

## Code Locations

| Component | File | Changes |
|---|---|---|
| LlmCache methods | `crates/crb-harness/src/cache.rs` | Add stats(), prune(), scrub(), backup(), restore(), rebuild() |
| CLI subcommands | `crates/crb-benchmark/src/main.rs` | Commands enum: CacheStats, CachePrune, CacheScrub, CacheBackup, CacheRestore, CacheRebuild; extend Clean with --outputs |
| Summary/history | `crates/crb-harness/src/lib.rs` | write_summary() → append to _runs.json |
| Helpers | `crates/crb-reporting/src/lib.rs` | Per-PR metadata writing helpers |

## Data Flow

### Cache Stats
```
cli → LlmCache::stats(base_dir) → walks all PR dirs
  → reads each index.json → computes per-PR & total stats
  → returns CacheStats struct → table or --json output
```

### Cache Prune
```
cli → LlmCache::prune(base_dir, options) → for each PR dir:
  → apply criteria: age (timestamp), --max-size, --max-prs
  → --dry-run: print what would be removed
  → actual: remove files, update index.json, remove empty PR dirs
```

### Cache Scrub
```
cli → LlmCache::scrub(base_dir) → for each PR dir:
  → read index.json → verify every file_path exists on disk
  → find orphan files (on disk but not in index)
  → detect/rebuild corrupted index.json from filesystem scan
```

### Clean --outputs
```
extend existing run_clean() → add --outputs flag that nukes output/ dir contents
```

### Cache Backup / Restore
```
backup: timestamped tarball of cache_dir → backup_{timestamp}.tar.gz
restore: extract tarball → overwrite cache_dir or restore to new location
```

### Run History (Batch 2)
```
write_summary() → also append entry to cache_dir/_runs.json array
  each entry: { run_id, timestamp, model, judge_model, total_prs, duration_secs, total_cost_usd, ... }
_runs.json is append-only, read/modify/write (small file, acceptable)
```

### Cache Rebuild (Batch 3, experimental)
```
cache-rebuild: iterate all index.json entries → compute new cache keys
  → if prompt hash migration needed → re-save entries with new keys
  → optional --dry-run
```

## Dependencies

No new external dependencies. Uses:
- `std::fs`, `std::path` for filesystem operations
- `serde_json` for JSON serialization
- `sha2` for content-addressed keying (already used)
- `flate2` + `tar` for backup (use `std::process::Command` for tar to avoid new deps)

## CLI Design

All new cache subcommands operate on `--cache-dir` (default: `cache`).
All destructive actions have `--dry-run`.
Output can be formatted as table (default) or `--json`.

```
crb-benchmark cache-stats [--cache-dir <path>] [--json]
crb-benchmark cache-prune [--cache-dir <path>] [--max-age <days>] [--max-size <bytes>] [--max-prs <n>] [--dry-run]
crb-benchmark cache-scrub [--cache-dir <path>] [--dry-run] [--repair]
crb-benchmark cache-backup [--cache-dir <path>] [--output <path>]
crb-benchmark cache-restore <backup_file> [--cache-dir <path>]
crb-benchmark cache-rebuild [--cache-dir <path>] [--dry-run]
crb-benchmark clean [--benchmark-dir <path>] [--all] [--outputs]
```

## Auto-backup on Run

When `--auto-backup` is passed to the `Run` subcommand, a timestamped backup is created before the run starts.
