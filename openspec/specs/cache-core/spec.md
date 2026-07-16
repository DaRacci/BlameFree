# cache-core Specification

## Purpose
Cache backend statistics and lifecycle management, including per-PR and global metrics for the LLM response cache.
## Requirements
### Requirement: Cache Statistics
The system SHALL provide cache statistics including per-PR and global metrics.

#### Scenario: Global stats
- GIVEN a cache directory with entries
- WHEN LlmCache::stats() is called
- THEN it returns GlobalCacheStats with pr_count, total_entries, total_size_bytes, and per-PR breakdowns

#### Scenario: Empty cache
- GIVEN an empty cache directory
- WHEN LlmCache::stats() is called
- THEN it returns stats with zero counts

### Requirement: Cache Pruning
The system SHALL prune cache entries based on configurable criteria.

#### Scenario: Prune by age
- GIVEN a max_age_days of 7
- WHEN LlmCache::prune() is called
- THEN it removes PRs where all entries are older than 7 days

#### Scenario: Prune by count
- GIVEN max_prs of 10
- WHEN LlmCache::prune() is called
- THEN it keeps the 10 newest PRs and removes the rest

#### Scenario: Prune by size
- GIVEN max_size_bytes of 1GB
- WHEN LlmCache::prune() is called
- THEN it removes oldest entries until total is under 1GB

#### Scenario: Dry run
- GIVEN dry_run=true
- WHEN LlmCache::prune() is called
- THEN it reports what would be removed without modifying the cache

### Requirement: Cache Scrubbing
The system SHALL reconcile cache index with filesystem state.

#### Scenario: Stale entry detection
- GIVEN an index.json referencing non-existent files
- WHEN LlmCache::scrub() is called
- THEN it reports stale entries

#### Scenario: Orphan file detection
- GIVEN files on disk not referenced in index
- WHEN LlmCache::scrub() is called
- THEN it reports orphan files

#### Scenario: Index repair
- GIVEN a corrupted index.json with repair=true
- WHEN LlmCache::scrub() is called
- THEN it rebuilds the index from the filesystem

### Requirement: Cache Backup and Restore
The system SHALL support tarball-based cache backup and restore.

#### Scenario: Backup
- GIVEN a populated cache directory
- WHEN LlmCache::backup() is called
- THEN it creates a timestamped tar.gz archive

#### Scenario: Restore
- GIVEN a backup tarball
- WHEN LlmCache::restore() is called
- THEN it extracts the tarball into the cache directory

### Requirement: Cache Rebuild
The system SHALL support rebuilding cache indices for prompt hash migrations.

#### Scenario: Rebuild with dry run
- GIVEN a cache with old-format keys
- WHEN LlmCache::rebuild(dry_run=true) is called
- THEN it reports what would change without modifying

### Requirement: Run History
The system SHALL record each benchmark run in an append-only history index.

#### Scenario: Record run
- GIVEN a completed benchmark run
- WHEN write_summary() is called
- THEN it appends a RunHistoryEntry to _runs.json

