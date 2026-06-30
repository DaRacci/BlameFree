# Cache Improvements — Tasks

## Status

- [x] **Proposal** written (openspec/cache-improvements/proposal.md)
- [x] **Design** written (openspec/cache-improvements/design.md)
- [x] **CLI spec** written (openspec/cache-improvements/specs/cache-cli/spec.md)
- [x] **Core spec** written (openspec/cache-improvements/specs/cache-core/spec.md)
- [x] **Implemented** — all improvements implemented

---

## Batch 1 (Priority 1)

### Task 1: `cache-stats` command
- [x] spec written
- [x] Add `CacheStats` variant to Commands enum in main.rs
- [x] Add `LlmCache::stats()` method in cache.rs that walks all PR directories
- [x] Add `CacheStats` struct for the aggregated data
- [x] Implement table output and `--json` flag
- [x] Tests: verify stats output with tempdir cache
- [x] Builds and tests pass

### Task 2: `cache-prune` command
- [x] spec written
- [x] Add `CachePrune` variant to Commands enum in main.rs
- [x] Add `LlmCache::prune()` method in cache.rs
- [x] Support `--max-age`, `--max-size`, `--max-prs` filters
- [x] Implement `--dry-run` mode (print what would be removed)
- [x] Tests: verify pruning behavior with tempdir cache
- [x] Builds and tests pass

### Task 3: `cache-scrub` command
- [x] spec written
- [x] Add `CacheScrub` variant to Commands enum in main.rs
- [x] Add `LlmCache::scrub()` method in cache.rs
- [x] Detect orphan files, stale entries, corrupted indices
- [x] Rebuild corrupted indices from filesystem scan
- [x] `--dry-run` and `--repair` flags
- [x] Tests: verify scrub behavior with deliberately corrupted cache
- [x] Builds and tests pass

### Task 4: `clean --outputs` extension
- [x] spec written
- [x] Add `--outputs` flag to Clean command in main.rs
- [x] Extend `run_clean()` in main.rs to remove output directories
- [x] Builds and tests pass

---

## Batch 2 (Priority 2)

### Task 5: `cache-backup` / `cache-restore` commands
- [x] spec written
- [x] Add `CacheBackup` and `CacheRestore` variants to Commands enum
- [x] Add `LlmCache::backup()` and `LlmCache::restore()` in cache.rs
- [x] Implement timestamped tarball using `tar` command
- [x] `--auto-backup` flag on Run subcommand
- [x] Tests: verify backup/restore round-trip
- [x] Builds and tests pass

### Task 6: Run history in `_runs.json`
- [x] spec written
- [x] Extend `write_summary()` to append to `_runs.json`
- [x] Define RunHistoryEntry struct
- [x] `_runs.json` format: array of run entries
- [x] Builds and tests pass

---

## Batch 3 (If time allows)

### Task 7: `cache-rebuild` command (experimental)
- [x] spec written
- [x] Add `CacheRebuild` variant to Commands enum
- [x] Add `LlmCache::rebuild()` method in cache.rs
- [x] Iterate all index entries and recompute cache keys
- [x] Handle prompt hash migration
- [x] `--dry-run` support
- [x] Tests: verify rebuild preserves data
- [x] Builds and tests pass
