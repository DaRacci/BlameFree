# crb-benchmark

CLI tool for code review benchmark preparation tasks — cloning repos, extracting diffs, and validating golden datasets.

- **`scaffold`** — Clone or fetch all benchmark repos referenced in a dataset
- **`fetch-diffs`** — Extract diffs from scaffolded repos for offline evaluation
- **`validate`** — Validate golden datasets for structural integrity
- **`list`** — List all PRs in a dataset with their URLs and titles

## CLI subcommands

```bash
# List all PRs in a dataset
cargo run -p crb-benchmark -- list --dataset-dir datasets/golden_comments

# Validate dataset integrity
cargo run -p crb-benchmark -- validate --dataset-dir datasets/golden_comments

# Clone/fetch all repos for a dataset
cargo run -p crb-benchmark -- scaffold --dataset-dir datasets/golden_comments --repos-dir repos

# Extract diffs from scaffolded repos
cargo run -p crb-benchmark -- fetch-diffs --repos-dir repos --output-dir diffs
```

## Key exports

- Uses `crb_reporting::load_golden_datasets` for dataset loading
- Uses `crb_reporting::GoldenCommentEntry` for PR data structures
