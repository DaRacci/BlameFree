# review-harness

> **⚠️ HEAVY ALPHA — NOT STABLE**
>
> This project is in very early, active development. Everything is broken, subject to change, and should not be relied upon. APIs, data formats, CLI flags, and core architecture are fluid. Use at your own risk.

Rust-powered code review harness. Evaluates AI code review agents against git diffs using multi-agent LLM reviewers.

## Quick start

```bash
# Set up your API key (OpenAI or OpenRouter)
export OPENAI_API_KEY="sk-..."
# For OpenRouter (optional):
export OPENAI_BASE_URL="https://openrouter.ai/api/v1"

# Review working-tree changes
cargo run --release --bin crb-harness -- review --working --model deepseek/deepseek-v4-pro
```

## CLI usage

```bash
# Review working-tree changes
cargo run --release --bin crb-harness -- review --working

# Review a specific commit range
cargo run --release --bin crb-harness -- review --commits HEAD~3..HEAD

# Review changes in a specific repo with a custom model
cargo run --release --bin crb-harness -- review \
  --working \
  --path /path/to/repo \
  --model gpt-4o
```

| Flag        | Env    | Default                       | Description                                 |
| ----------- | ------ | ----------------------------- | ------------------------------------------- |
| `--working` | —      | `false`                       | Review working-tree changes (unstaged + staged) |
| `--commits` | —      | —                             | Commit range to review (format: base..head) |
| `--path`    | —      | `.`                           | Path to the git repository                  |
| `--model`   | `MODEL`| `deepseek/deepseek-v4-pro`    | Model for agent reviews                     |
