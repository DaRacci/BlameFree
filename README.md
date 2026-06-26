# review-harness

Rust-powered code review benchmark harness. Evaluates AI code review agents
against golden comment datasets using LLM-as-judge.

## Quick Start

```bash
# Set up your API key (OpenAI or OpenRouter)
export OPENAI_API_KEY="sk-..."
# For OpenRouter (optional):
export OPENAI_BASE_URL="https://openrouter.ai/api/v1"

# Run the harness
cargo run --release -- -p crb-harness -- --model deepseek/deepseek-v4-pro --judge-model deepseek/deepseek-v4-flash
```

## CLI Options

| Flag            | Env           | Default                      | Description                         |
| --------------- | ------------- | ---------------------------- | ----------------------------------- |
| `--dataset-dir` | `DATASET_DIR` | `datasets/golden_comments`   | Golden comments dataset directory   |
| `--repos-dir`   | `REPOS_DIR`   | `repos`                      | Pre-scaffolded repos directory      |
| `--output-dir`  | `OUTPUT_DIR`  | `output`                     | Output directory for results        |
| `--model`       | `MODEL`       | `deepseek/deepseek-v4-pro`   | Model for agent reviews             |
| `--judge-model` | `JUDGE_MODEL` | `deepseek/deepseek-v4-flash` | Model for judge evaluation          |
| `--concurrency` | `CONCURRENCY` | `4`                          | Max concurrent PR evaluations       |
| `--dry-run`     | —             | `false`                      | Load config and datasets, then exit |
| `--resume`      | —             | `false`                      | Skip PRs with existing result files |

## Dry Run

```bash
cargo run -- -p crb-harness -- --dry-run
```

## Project Structure

## Architecture

- **Agents**: 4 concurrent LLM agents (SA=static analysis, CL=code logic, AR=architecture, SEC=security) review each PR diff.
- **Judge**: An LLM compares each agent finding against golden comments using the Martian JUDGE_PROMPT and returns a verdict (match + confidence).
- **Metrics**: Precision, recall, and F1 are computed per PR, per language, and overall.
- **Output**: Per-PR JSON files + summary CSV in the output directory.
