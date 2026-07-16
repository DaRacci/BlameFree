# Cache Improvements — Proposal

## Status
**Draft** — Proposed improvements to the CRB benchmark cache system.

## Why

The existing `LlmCache` implementation provides content-addressed caching for LLM interactions during PR evaluation, but lacks operational tooling. Users have no visibility into cache contents, no way to manage cache size, and no means to recover from corruption. As benchmarks grow to hundreds of PRs, cache directories can consume significant disk space without any management facilities.

## What Changes

Add six new CLI subcommands to `crb-benchmark` (`cache-stats`, `cache-prune`, `cache-scrub`, `cache-backup`, `cache-restore`, `cache-rebuild`) and corresponding `LlmCache` methods. Extend the `Clean` subcommand with `--outputs` flag and `Run` subcommand with `--auto-backup` flag. Add append-only `_runs.json` history file.

## Goals

1. **Observability**: Operators can inspect cache contents (PRs, entries, sizes, hit rates) via CLI.
2. **Lifecycle management**: Operators can prune old/stale entries, scrub corrupted indices, and clean output directories.
3. **Backup/restore**: Cache snapshots can be created and restored for CI reproducibility.
4. **Run history**: Each benchmark run is recorded in an append-only history index.
5. **Future-proofing**: Cache rebuild tooling for prompt hash migrations.

## Non-Goals

- Backward compatibility with old `_summary.json` format (breaking change accepted).
- Runtime performance optimization of the cache itself.
- Distributed or networked cache backends.

## Project
`/data/workspace/projects/review-harness`
