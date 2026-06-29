#!/bin/bash
# Smoke test: 1 PR against Python+Hermes baseline
# Run from: /data/workspace/projects/review-harness
#
# This script runs the Rust harness on a single PR (discourse-graphite/pull/1)
# and compares the results to the Python+Hermes baseline.
#
# Usage: bash scripts/smoke-test-1pr.sh
set -euo pipefail

cd "$(dirname "$0")/.."

SMOKE_DIR="output/smoke-test-1pr-fresh"
BASELINE_DIR="/data/workspace/projects/code-review-benchmark-research/datasets/code-review-benchmark/offline/results/deepseek_deepseek-v4-flash"

echo "=== Smoke Test: 1 PR vs Python+Hermes Baseline ==="
echo "PR: discourse-graphite/pull/1 (FEATURE: automatically downsize large images)"
echo ""

# 1. Run the harness
echo "--- Running Rust harness ---"
RUST_LOG=info cargo run --bin crb-harness --release -- \
    --model "deepseek/deepseek-v4-flash" \
    --judge-model "deepseek/deepseek-v4-flash" \
    --dataset-dir "datasets/golden_comments" \
    --roles "SA" \
    --concurrency 1 \
    --max-findings 5 \
    --prompts-dir "prompts/builtin" \
    --skip-consensus \
    --pr-filter "discourse-graphite/pull/1" \
    --output-dir "${SMOKE_DIR}"

echo ""
echo "=== Rust Harness Results ==="
RUST_RESULT="${SMOKE_DIR}/FEATURE__automatically_downsize_large_images.json"
if [ -f "${RUST_RESULT}" ]; then
    cat "${RUST_RESULT}" | python3 -m json.tool
else
    echo "No result file found at ${RUST_RESULT}"
    ls "${SMOKE_DIR}/" 2>/dev/null || echo "Output dir empty"
fi

echo ""
echo "=== Python+Hermes Baseline Results ==="
echo "From ${BASELINE_DIR}/evaluations.json:"
python3 -c "
import json
with open('${BASELINE_DIR}/evaluations.json') as f:
    data = json.load(f)
pr_url = 'https://github.com/ai-code-review-evaluation/discourse-graphite/pull/1'
entry = data.get(pr_url, {}).get('hermes', {})
print(json.dumps(entry, indent=2))
"

echo ""
echo "=== Comparison ==="
python3 -c "
import json

# Rust results
with open('${RUST_RESULT}') as f:
    rust = json.load(f)
rm = rust.get('metrics', {})
rc = rust.get('cost', {})

# Baseline
with open('${BASELINE_DIR}/evaluations.json') as f:
    data = json.load(f)
pr_url = 'https://github.com/ai-code-review-evaluation/discourse-graphite/pull/1'
base = data.get(pr_url, {}).get('hermes', {})

print(f'{\"Metric\":<25} {\"Rust Harness\":>15} {\"Python Baseline\":>15}')
print(f'{\"-\"*25} {\"-\"*15} {\"-\"*15}')
print(f'{\"True Positives\":<25} {rm.get(\"true_positives\", \"N/A\"):>15} {base.get(\"tp\", \"N/A\"):>15}')
print(f'{\"False Positives\":<25} {rm.get(\"false_positives\", \"N/A\"):>15} {base.get(\"fp\", \"N/A\"):>15}')
print(f'{\"False Negatives\":<25} {rm.get(\"false_negatives\", \"N/A\"):>15} {base.get(\"fn\", \"N/A\"):>15}')
print(f'{\"Precision\":<25} {rm.get(\"precision\", 0)*100:>14.1f}% {base.get(\"precision\", 0)*100:>14.1f}%')
print(f'{\"Recall\":<25} {rm.get(\"recall\", 0)*100:>14.1f}% {base.get(\"recall\", 0)*100:>14.1f}%')
print(f'{\"F1 Score\":<25} {rm.get(\"f1\", 0)*100:>14.1f}% {base.get(\"f1\", 0)*100:>14.1f}%')
print(f'{\"Cost (USD)\":<25} {\$\${rc.get(\"total_usd\", 0):>13.6f}} {\"N/A\":>15}')
print(f'{\"Golden Comments\":<25} {rust.get(\"golden_count\", 0):>15} {base.get(\"total_golden\", \"N/A\"):>15}')
print(f'{\"Findings\":<25} {rust.get(\"findings_count\", 0):>15} {base.get(\"total_candidates\", \"N/A\"):>15}')
"
