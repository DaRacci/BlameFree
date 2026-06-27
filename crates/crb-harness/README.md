# crb-harness

Main CLI binary for the code review benchmark harness — orchestrates the end-to-end pipeline of loading datasets, running multi-agent LLM reviewers, judging findings against golden comments, and producing reports.

- Loads golden comment datasets from JSON files and evaluates LLM reviewer agents against them
- Supports concurrent evaluation of multiple PRs with configurable agents (SA, CL, AR, SEC roles)
- Includes TUI dashboard (`--dashboard`), JSON event streaming (`--dashboard-events`), LLM caching, linter integration, and CI validation mode

## Key types

- [`CliArgs`](src/config.rs) — All CLI flags: `--model`, `--judge-model`, `--concurrency`, `--output-dir`, `--dataset-dir`, `--pr-filter`, `--cache-dir`, `--dry-run`, `--resume`, `--skip-consensus`, `--linters-only`, `--ci`, `--dashboard`, `--dashboard-events`, `--rules-dir`, `--prompts-dir`, `--validate`, `--roles`, `--max-findings`, `--repos-dir`

## CLI usage

```bash
# Evaluate a subset of PRs with custom prompts and LLM caching
cargo run --bin crb-harness -- \
  --pr-filter "discourse-graphite/pull/7" \
  --prompts-dir prompts/builtin \
  --cache-dir cache/test

# Full run with 8 concurrent PRs, TUI dashboard, and CI validation
cargo run --bin crb-harness -- \
  --model deepseek/deepseek-v4-flash \
  --concurrency 8 \
  --dashboard \
  --ci

# Dry run — print what would be evaluated without making API calls
cargo run --bin crb-harness -- --dry-run

# Resume a partially-completed run (skips PRs with existing output files)
cargo run --bin crb-harness -- --resume
```
