#!/usr/bin/env bash
# 50-PR baseline run for review-harness (v6-baseline)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

RUN_NAME="v6-baseline"
OUTPUT_DIR="output/${RUN_NAME}"
mkdir -p "$OUTPUT_DIR"

export PATH="/usr/bin:$PATH"
export RUST_LOG="info"

echo "[$(date)] Starting $RUN_NAME (50 PRs)..."

cargo run --bin crb-harness -- \
    --repos-dir /data/workspace/projects/code-review-benchmark-research/datasets/code-review-benchmark/offline/repos \
    --prompts-dir prompts/builtin \
    --model deepseek/deepseek-v4-flash \
    --judge-model deepseek/deepseek-v4-flash \
    --concurrency 4 \
    --output-dir "$OUTPUT_DIR" \
    --max-findings 20 \
    --resume \
    2>&1 | tee "${OUTPUT_DIR}/run.log"

echo "[$(date)] $RUN_NAME completed — exit code $?"
