# crb-harness

CLI binary for reviewing git diffs using multi-agent LLM reviewers — evaluates working-tree changes or commit ranges and produces structured findings.

- Reviews working-tree changes (`--working`) or a commit range (`--commits base..head`)
- Runs a configurable LLM agent (via `--model`) against the diff
- Integrates with `crb-agents`, `crb-consensus`, `crb-reporting`, and other workspace crates for the full review pipeline

## Key types

- [`Cli`](src/config.rs) — Top-level CLI enum with a `Review` subcommand
- [`ReviewArgs`](src/config.rs) — Review-specific flags: `--commits`, `--working`, `--path`, `--model`

## CLI usage

```bash
# Review working-tree changes
cargo run --bin crb-harness -- review --working

# Review a specific commit range
cargo run --bin crb-harness -- review --commits HEAD~3..HEAD

# Review changes in a specific repo with a custom model
cargo run --bin crb-harness -- review \
  --working \
  --path /path/to/repo \
  --model gpt-4o
```
