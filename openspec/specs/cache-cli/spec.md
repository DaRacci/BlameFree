# cache-cli Specification

## Purpose
CLI subcommands for inspecting, pruning, backing up, and restoring the content-addressed LLM response cache.
## Requirements
### Requirement: CacheStats Subcommand
The system SHALL provide a cache-stats CLI subcommand.

#### Scenario: Show stats
- GIVEN a cache directory
- WHEN cache-stats is invoked
- THEN it displays PR count, entry count, total size, and per-PR breakdown
- AND it supports --json output flag

### Requirement: CachePrune Subcommand
The system SHALL provide a cache-prune CLI subcommand.

#### Scenario: Prune with criteria
- GIVEN --max-age, --max-size, or --max-prs options
- WHEN cache-prune is invoked
- THEN it prunes the cache according to those criteria
- AND it supports --dry-run and --json flags

### Requirement: CacheScrub Subcommand
The system SHALL provide a cache-scrub CLI subcommand.

#### Scenario: Scrub cache
- GIVEN --dry-run or --repair options
- WHEN cache-scrub is invoked
- THEN it reconciles index.json with the filesystem
- AND it supports --json output

### Requirement: CacheBackup Subcommand
The system SHALL provide a cache-backup CLI subcommand.

#### Scenario: Create backup
- GIVEN --output path
- WHEN cache-backup is invoked
- THEN it creates a timestamped tarball at the specified path

### Requirement: CacheRestore Subcommand
The system SHALL provide a cache-restore CLI subcommand.

#### Scenario: Restore from backup
- GIVEN a backup file path
- WHEN cache-restore is invoked
- THEN it extracts the tarball into the cache directory

### Requirement: CacheRebuild Subcommand
The system SHALL provide a cache-rebuild CLI subcommand (experimental).

#### Scenario: Rebuild with dry run
- GIVEN --dry-run flag
- WHEN cache-rebuild is invoked
- THEN it reports index changes without modifying

### Requirement: Clean --outputs Extension
The system SHALL extend the clean subcommand with --outputs flag.

#### Scenario: Clean outputs
- GIVEN --outputs flag
- WHEN clean is invoked
- THEN it also removes output directories

### Requirement: Run --auto-backup Extension
The system SHALL extend the run subcommand with --auto-backup flag.

#### Scenario: Auto-backup before run
- GIVEN --auto-backup flag
- WHEN run is invoked
- THEN it backs up the cache before starting the run

