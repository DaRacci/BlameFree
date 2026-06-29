# Smoke Test: 1 PR vs Python+Hermes Baseline

## PR Under Test

**PR**: [discourse-graphite/pull/1](https://github.com/ai-code-review-evaluation/discourse-graphite/pull/1)  
**Title**: "FEATURE: automatically downsize large images"  
**Original**: [discourse/discourse commit ffbaf8c](https://github.com/discourse/discourse/commit/ffbaf8c54269df2ce510de91245760fddce09896)

## Golden Comments (3)

| # | Comment | Severity |
|---|---------|----------|
| 1 | The downsize method is defined twice. The second definition, which expects a single dimensions string parameter, overrides the first, which expected separate max_width and max_height parameters. This makes the original method unreachable and breaks existing code that calls it with separate width and height arguments. | Medium |
| 2 | Hardcoding maxSizeKB = 10 \* 1024 ignores Discourse.SiteSettings['max_' + type + '_size_kb'], so the client-side limit can diverge from server-side and per-type settings (also applies to the 413 handler below). | Low |
| 3 | Passing 80% as the dimensions can fail for animated GIFs when allow_animated_thumbnails is true, since the animated path uses gifsicle --resize-fit which expects WxH geometry, not a percentage; downsizing would then silently fail. | Medium |

## Baseline: Python+Hermes (evaluations.json)

Source: `/data/workspace/projects/code-review-benchmark-research/datasets/code-review-benchmark/offline/results/deepseek_deepseek-v4-flash/evaluations.json`

The Python+Hermes pipeline (v4.7 orchestrator) produced the following on discourse-graphite/pull/1:

| Metric | Value |
|--------|-------|
| True Positives | 0 |
| False Positives | 0 |
| False Negatives | 3 |
| Precision | 0.0% |
| Recall | 0.0% |
| F1 Score | 0.0% |
| Total Candidates | 0 |
| Total Golden | 3 |

All 3 golden comments were missed — the pipeline produced 0 findings for this PR.

## Smoke Test: Rust Harness

### Method

```bash
cd /data/workspace/projects/review-harness

# Run single-agent SA, skip consensus, built-in prompts
./target/debug/crb-harness \
    --model "deepseek/deepseek-v4-flash" \
    --judge-model "deepseek/deepseek-v4-flash" \
    --dataset-dir "datasets/golden_comments" \
    --roles "SA" \
    --concurrency 1 \
    --max-findings 5 \
    --prompts-dir "prompts/builtin" \
    --skip-consensus \
    --pr-filter "discourse-graphite/pull/1" \
    --output-dir "output/smoke-test-1pr-fresh"

# Run with EXP-013 v6 baseline prompts (4 agents)
./target/debug/crb-harness \
    --model "deepseek/deepseek-v4-flash" \
    --judge-model "deepseek/deepseek-v4-flash" \
    --dataset-dir "datasets/golden_comments" \
    --roles "SA,CL,AR,SEC" \
    --concurrency 1 \
    --max-findings 5 \
    --prompts-dir "experiments/EXP-013/prompts" \
    --pr-filter "discourse-graphite/pull/1" \
    --output-dir "output/smoke-test-1pr-exp013"
```

### Results (2026-06-29)

#### Run 1: Single SA agent, built-in prompts, `--skip-consensus`

| Metric | Rust Harness | Python Baseline |
|--------|-------------|-----------------|
| True Positives | 0 | 0 |
| False Positives | 3 | 0 |
| False Negatives | 3 | 3 |
| Precision | 0.0% | 0.0% |
| Recall | 0.0% | 0.0% |
| F1 Score | 0.0% | 0.0% |
| Findings | 1 | 0 |
| Cost | ~$0.00017 | N/A |

The model returned a single meta-finding: "no code diff was provided" — the LLM was unable to access the PR diff for analysis. This suggests the repos directory needs to be pre-scaffolded with the discourse-graphite repository.

#### Run 2: 4 agents (SA,CL,AR,SEC), EXP-013 prompts (v6 baseline)

To be populated after run completes.

#### Previous Run (ca-test-1): Rust Harness with v6 baseline

A prior run at `output/ca-test-1/` produced:

| Metric | Rust Harness | Python Baseline |
|--------|-------------|-----------------|
| True Positives | 1 | 0 |
| False Positives | 2 | 0 |
| False Negatives | 2 | 3 |
| Precision | 33.3% | 0.0% |
| Recall | 33.3% | 0.0% |
| F1 Score | **33.3%** | 0.0% |
| Findings | 3 verdicts (1 TP, 2 FP) | 0 |

This shows the Rust harness can meaningfully outperform the Python baseline when properly configured with the EXP-013 prompts.

## What to Compare

When running the smoke test, compare:

1. **Findings found** vs 0 in the Python baseline — even 1 finding is an improvement
2. **TP/FP/FN**: True positives indicate actual bugs caught; FPs are noise
3. **F1 Score**: Harmonic mean of precision and recall
4. **Cost**: LLM API cost per PR (the Python baseline didn't track this)
5. **Token usage**: Efficiency of the agent prompts

## How to Run

```bash
# Full smoke test with comparison
bash /data/workspace/projects/review-harness/scripts/smoke-test-1pr.sh
```

## Expected Results

- **Rust harness** should match or exceed the Python baseline (0 TP, 0 FP, 3 FN, F1=0.0%)
- With EXP-013 prompts and pre-scaffolded repos, expect TP≥1, F1≥15%
- Without scaffolding, expect identical results to baseline (no code diff → no findings)

## Issues Encountered

1. **No pre-scaffolded repos**: The `repos/` directory is empty. The scaffold command is a placeholder that doesn't actually clone. Manual cloning of discourse-graphite may be needed.
2. **Model returns empty findings**: Without code diffs, the model cannot analyze the PR.
3. **Python baseline had 0 findings for this PR**: The discourse PR 1 dataset was not well-covered by the original Python pipeline.
