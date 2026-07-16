# judge Specification

## Purpose
Judge evaluation logic for comparing agent findings against golden comments with precision, recall, and F1 calculation.
## Requirements
### Requirement: Precision/Recall Calculation
- The system SHALL compute precision (TP / (TP + FP)) and recall (TP / (TP + FN)) from judge decisions.
- (Previously calculated via Python script pipeline. Now computed directly from Rust judge output via rig Extractor.)

#### Scenario: Standard precision/recall computation
- GIVEN a set of TP, FP, FN counts from judge decisions
- WHEN the system computes metrics
- THEN it outputs precision = TP / (TP + FP), recall = TP / (TP + FN), and F1 = 2 * (precision * recall) / (precision + recall)

