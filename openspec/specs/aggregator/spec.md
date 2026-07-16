# aggregator Specification

## Purpose
Parsing, deduplication, and aggregation of benchmark findings from linter and LLM agents into unified review results.
## Requirements
### Requirement: Severity Classification
The system SHALL normalise arbitrary severity strings to the `Severity` enum via exact match, abbreviation match, and prefix fallback.

#### Scenario: Exact match
- GIVEN severity text `"critical"`, `"high"`, `"medium"`, or `"low"`
- WHEN `classify_severity()` is called
- THEN it returns the corresponding `Severity` variant (case-insensitive)

#### Scenario: Abbreviation match
- GIVEN `"crit"` or `"med"`
- WHEN `classify_severity()` is called
- THEN it returns `Severity::Critical` or `Severity::Medium` respectively

#### Scenario: Prefix fallback
- GIVEN `"critical!!!"`, `"high severity"`, `"med priority"`, or `"low impact"`
- WHEN `classify_severity()` is called
- THEN it matches on prefix: `"crit"`, `"high"`, `"med"`, `"low"`

#### Scenario: Default to Medium
- GIVEN empty string or unrecognised input
- WHEN `classify_severity()` is called
- THEN it returns `Severity::Medium`

### Requirement: Text Normalisation
The system SHALL normalise finding text for comparison by lowercasing, stripping markdown formatting, and collapsing whitespace.

#### Scenario: Markdown stripping
- GIVEN text `"**Critical**: \`bug\` in #module"`
- WHEN `normalize()` is called
- THEN it returns `"critical: bug in module"`

### Requirement: Function Name Extraction
The system SHALL extract function/method/class names from finding text using a series of regex patterns in priority order.

#### Scenario: Standard patterns
- GIVEN text `"function foo() has a bug"` or `` "method `bar` is unused" `` or `"in file.py#func"` or `` "`my_func`" ``
- WHEN `extract_function()` is called
- THEN it returns the extracted name (with `"file.func"` format where applicable)

#### Scenario: No match
- GIVEN text `"general observation about code quality"`
- WHEN `extract_function()` is called
- THEN it returns `None`

### Requirement: Jaccard Similarity
The system SHALL compute Jaccard similarity between two texts using word-level set intersection/union.

#### Scenario: Identical texts
- GIVEN `"bug in login"` and `"bug in login"`
- WHEN `jaccard_similarity()` is called
- THEN it returns `1.0`

#### Scenario: Disjoint texts
- GIVEN `"security vulnerability"` and `"naming convention style"`
- THEN it returns `0.0`

#### Scenario: Partial overlap
- GIVEN `"bug in login function"` and `"bug in auth function"`
- THEN the result is between `0.0` and `1.0`

### Requirement: Semantic Dedup
The system SHALL deduplicate findings using a three-tier strategy: (1) same file+function, (2) same file+line bucket, (3) Jaccard text similarity at 0.4 threshold.

#### Scenario: Same function merge
- GIVEN two findings in the same file, same extracted function name
- WHEN `semantic_dedup()` is called
- THEN it returns one finding with combined cross-validation count from both

#### Scenario: Richest finding kept
- GIVEN multiple findings merged in one group
- WHEN `semantic_dedup()` selects the best
- THEN it keeps the finding with the longest text, prioritising those with line numbers and evidence

#### Scenario: Jaccard threshold dedup
- GIVEN ungrouped findings with Jaccard similarity >= 0.4
- WHEN `semantic_dedup()` runs
- THEN they are merged into a single finding

### Requirement: Multi-Format Report Parsing
The system SHALL parse Phase 4 report text using three strategies in priority order: (a) table-row format, (b) bullet-list/prose format, (c) JSON fallback.

#### Scenario: Table-row parsing
- GIVEN a report with `### 🔴 C1 — Title` headings and `| **Field** | Value |` rows
- WHEN `parse_report()` is called
- THEN it extracts finding ID, text, severity, path, line, evidence, and cross-validation info

#### Scenario: Bullet-list parsing
- GIVEN a report with `### CRITICAL Findings` sections and `- **C1**: description` bullets
- WHEN `parse_report()` is called
- THEN it extracts findings with the section's severity

#### Scenario: JSON fallback parsing
- GIVEN a report containing a JSON array or `{"findings": [...]}` object
- WHEN `parse_report()` is called
- THEN it extracts findings from the JSON structure

#### Scenario: Stop at notes sections
- GIVEN a report with a `## Notes` or `## pre-existing` section
- WHEN parsing table format
- THEN it stops processing at that section boundary

#### Scenario: Empty input
- GIVEN empty or garbage text
- WHEN `parse_report()` is called
- THEN it returns an empty `Vec`

### Requirement: Candidate Formatting
The system SHALL format findings as `Candidate` structs with severity badge, cross-validation label, and source tag.

#### Scenario: Basic formatting
- GIVEN a `Finding` with `Severity::High`, `cross_validated_by = 1`
- WHEN `format_candidate()` is called
- THEN it returns a `Candidate` with text `"[High] description"`, source `"orchestrator_phase4"`

#### Scenario: Cross-validated badge
- GIVEN a `Finding` with `cross_validated_by = 2`
- WHEN `format_candidate()` is called
- THEN the text includes `" [cross-validated]"` after the severity badge

### Requirement: Batch Aggregation
The system SHALL process multiple reports through the full pipeline: parse -> dedup -> sort -> cap -> format.

#### Scenario: Single PR pipeline
- GIVEN a `Map` with one PR URL and its report text
- WHEN `aggregate_batch()` is called
- THEN it returns parsed, deduped, sorted, capped, and formatted candidates with stats

