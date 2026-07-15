# crb-harness

CLI binary for running the review-harness evaluation pipeline — evaluating PRs using multi-agent LLM reviewers with structured findings.

- Runs the full evaluation pipeline (agents, consensus, reporting) via programmatic config
- Integrates with `crb-agents`, `crb-consensus`, `crb-reporting`, and other workspace crates

## CLI usage

The `review` subcommand has been removed. Use `crb_harness::pipeline::evaluate` directly.
