# Change: MVP Core Harness

## Intent
Replace Hermes subagent orchestration with a standalone Rust benchmark harness using rig-core for LLM API calls, eliminating the variance and overhead of multi-agent spawning for code review evaluation.

## Scope
Build the core evaluation loop: load golden comments → run concurrent agent prompts (SA, CL, AR, SEC) → judge results against golden comments → aggregate precision/recall/F1.

Out of scope for this change: linter subprocess calls, web dashboard, multi-model calibration.

## Approach
Rust + rig-core 0.39 + tokio + clap. ~1000 lines total across 5 modules. Keep Martian golden_comments/ datasets (MIT license) and JUDGE_PROMPT as-is.
