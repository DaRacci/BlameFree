# crb-judge

LLM-as-judge evaluation for code review findings — compares candidate findings against golden (expected) comments using a structured judge agent.

- [`run_judge()`] prompts a judge LLM with a golden comment and a candidate finding, returning a structured [`JudgeVerdict`] (reasoning, match, confidence)
- [`compute_metrics()`] translates a list of verdicts into precision, recall, and F1 scores against a known golden count
- Built-in `JUDGE_PROMPT` template with `{golden_comment}` and `{candidate}` placeholders

## Key types

- [`JudgeVerdict`](src/lib.rs) — `{ reasoning, match_, confidence }` returned by the judge LLM
- [`Metrics`](src/lib.rs) — `{ true_positives, false_positives, false_negatives, precision, recall, f1 }`
- [`build_judge()`](src/lib.rs) — Creates a Rig `Agent` with the `JUDGE_PROMPT` as its preamble
- [`format_judge_prompt()`](src/lib.rs) — Formats the judge prompt with substituted values
