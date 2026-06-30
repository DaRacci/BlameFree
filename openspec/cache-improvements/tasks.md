# Cache Improvements — Tasks

## Status

- [x] **Proposal** written (openspec/cache-improvements/proposal.md)
- [x] **Design** written (openspec/cache-improvements/design.md)
- [x] **CLI spec** written (openspec/cache-improvements/specs/cache-cli/spec.md)
- [x] **Core spec** written (openspec/cache-improvements/specs/cache-core/spec.md)
- [ ] **Implemented** — delegating to leaf subagents

---

## Batch 1 (Priority 1)

### Task 1: `cache-stats` command
- [x] spec written
- [ ] Add `CacheStats` variant to Commands enum in main.rs
- [ ] Add `LlmCache::stats()` method in cache.rs that walks all PR directories
- [ ] Add `CacheStats` struct for the aggregated data
- [ ] Implement table output and `--json` flag
- [ ] Tests: verify stats output with tempdir cache
- [ ] Builds and tests pass

### Task 2: `cache-prune` command
- [x] spec written
- [ ] Add `CachePrune` variant to Commands enum in main.rs
- [ ] Add `LlmCache::prune()` method in cache.rs
- [ ] Support `--max-age`, `--max-size`, `--max-prs` filters
- [ ] Implement `--dry-run` mode (print what would be removed)
- [ ] Tests: verify pruning behavior with tempdir cache
- [ ] Builds and tests pass

### Task 3: `cache-scrub` command
- [x] spec written
- [ ] Add `CacheScrub` variant to Commands enum in main.rs
- [ ] Add `LlmCache::scrub()` method in cache.rs
- [ ] Detect orphan files, stale entries, corrupted indices
- [ ] Rebuild corrupted indices from filesystem scan
- [ ] `--dry-run` and `--repair` flags
- [ ] Tests: verify scrub behavior with deliberately corrupted cache
- [ ] Builds and tests pass

### Task 4: `clean --outputs` extension
- [x] spec written
- [ ] Add `--outputs` flag to Clean command in main.rs
- [ ] Extend `run_clean()` in main.rs to remove output directories
- [ ] Tests: verify outputs dir removal
- [ ] Builds and tests pass

---

## Batch 2 (Priority 2)

### Task 5: `cache-backup` / `cache-restore` commands
- [x] spec written
- [ ] Add `CacheBackup` and `CacheRestore` variants to Commands enum
- [ ] Add `LlmCache::backup()` and `LlmCache::restore()` in cache.rs
- [ ] Implement timestamped tarball using `tar` command
- [ ] `--auto-backup` flag on Run subcommand
- [ ] Tests: verify backup/restore round-trip
- [ ] Builds and tests pass

### Task 6: Run history in `_runs.json`
- [x] spec written
- [ ] Extend `write_summary()` to append to `_runs.json`
- [ ] Define RunHistoryEntry struct
- [ ] `_runs.json` format: array of run entries
- [ ] Tests: verify history accumulates across runs
- [ ] Builds and tests pass

---

## Batch 3 (If time allows)

### Task 7: `cache-rebuild` command (experimental)
- [x] spec written
- [ ] Add `CacheRebuild` variant to Commands enum
- [ ] Add `LlmCache::rebuild()` method in cache.rs
- [ ] Iterate all index entries and recompute cache keys
- [ ] Handle prompt hash migration
- [ ] `--dry-run` support
- [ ] Tests: verify rebuild preserves data
- [ ] Builds and tests pass
