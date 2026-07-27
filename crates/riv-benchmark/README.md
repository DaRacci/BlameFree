# riv-benchmark

CLI tool for benchmarking this project for performance and inference quality.

## CLI subcommands

```bash
# List all PRs in a dataset
cargo run -p riv-benchmark -- list --dataset-dir datasets/golden_comments

# Validate dataset integrity
cargo run -p riv-benchmark -- validate --dataset-dir datasets/golden_comments

# Clone/fetch all repos for a dataset
cargo run -p riv-benchmark -- scaffold --dataset-dir datasets/golden_comments --repos-dir repos

# Extract diffs from scaffolded repos
cargo run -p riv-benchmark -- fetch-diffs --repos-dir repos --output-dir diffs
```
