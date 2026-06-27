# Delta for Judge

## ADDED Requirements

### Requirement: LLM-as-Judge Evaluation
The system SHALL compare agent findings against golden comments using an LLM judge.

#### Scenario: Semantic matching
- GIVEN a golden comment and a candidate finding from an agent
- WHEN the judge evaluates them
- THEN it calls an LLM with the Martian JUDGE_PROMPT template
- AND it returns a JSON verdict: {reasoning, match: bool, confidence: float}
- AND it classifies the finding as TP (match=true), FP (match=false), or FN (missed golden)

#### Scenario: Ambiguous handling
- GIVEN a candidate that partially matches a golden comment
- WHEN confidence is < 0.5
- THEN the judge records it as a non-match (FP) with low confidence noted
- BUT the raw verdict is preserved in per-PR results for manual audit

### Requirement: Judge Configuration
The system SHALL support configurable judge model, temperature, and structured output mode.

#### Scenario: Model override
- GIVEN a command-line flag `--judge-model deepseek/deepseek-v4-flash`
- WHEN the harness runs
- THEN all judge calls use the specified model instead of the default

## MODIFIED Requirements

### Requirement: Precision/Recall Calculation
- The system SHALL compute precision (TP / (TP + FP)) and recall (TP / (TP + FN)) from judge decisions.
- (Previously calculated via Python script pipeline. Now computed directly from Rust judge output via rig Extractor.)
