# severity-auditor Specification

## Purpose
Severity inflation classification and enforcement of never-downgrade rules to ensure consistent severity assignment across review findings.
## Requirements
### Requirement: Severity Value Mapping
The system SHALL map `Severity` enum variants to numeric values for comparison, where lower = more severe: Critical=0, High=1, Medium=2, Low=3.

#### Scenario: Numeric ordering
- GIVEN `Severity::Critical`
- WHEN `severity_value()` is called
- THEN it returns `0`
- AND `High < Medium < Low` in numeric order

### Requirement: Inflated Pattern Detection
The system SHALL detect inflated severity labels by matching finding text and evidence against three categories of patterns, returning the first matching category.

#### Scenario: Architecture nits match
- GIVEN a finding with text `"SRP violation in UserService"`
- WHEN `has_inflated_pattern()` is called
- THEN it returns `Some(InflatedCategory::ArchitectureNits)` (quantum -2)

#### Scenario: Hypothetical/theoretical match
- GIVEN a finding with text `"Could cause a performance issue"`
- WHEN `has_inflated_pattern()` is called
- THEN it returns `Some(InflatedCategory::HypotheticalTheoretical)` (quantum -1)

#### Scenario: Style nits match
- GIVEN a finding with text `"Naming convention is inconsistent"`
- WHEN `has_inflated_pattern()` is called
- THEN it returns `Some(InflatedCategory::StyleNits)` (quantum -3)

#### Scenario: No match
- GIVEN a finding with no inflated language
- WHEN `has_inflated_pattern()` is called
- THEN it returns `None`

### Requirement: Never-Downgrade Protection
The system SHALL protect genuine security, data-integrity, and correctness bugs from any severity downgrade, even if they also match inflated patterns.

#### Scenario: Security vulnerability protection
- GIVEN a finding with text `"SQL injection vulnerability in login query"`
- WHEN `has_never_downgrade_pattern()` is called
- THEN it returns `Some("security_vulns")`

#### Scenario: Data integrity protection
- GIVEN a finding with text `"Race condition in cache update logic"`
- WHEN `has_never_downgrade_pattern()` is called
- THEN it returns `Some("data_integrity")`

#### Scenario: Correctness bug protection
- GIVEN a finding with text `"Null pointer exception possible"`
- WHEN `has_never_downgrade_pattern()` is called
- THEN it returns `Some("correctness_bugs")`

#### Scenario: Protection in evidence
- GIVEN a finding where the pattern appears in evidence text, not main text
- WHEN `has_never_downgrade_pattern()` is called
- THEN it still matches

#### Scenario: No protection needed
- GIVEN a finding with style/text pattern only
- WHEN `has_never_downgrade_pattern()` is called
- THEN it returns `None`

### Requirement: Severity Downgrade Application
The system SHALL apply severity downgrades deterministically through a four-step pipeline, only ever downgrading (never upgrading).

#### Scenario: Full pipeline — protected finding
- GIVEN a finding with text `"SQL injection in query"`, severity `Critical`
- WHEN `apply_severity_auditor()` is called
- THEN the finding's severity stays `Critical`
- AND `severity_audited = false`
- AND `severity_audit_reason` starts with `"protected_by_never_downgrade_pattern"`

#### Scenario: Full pipeline — multi-agent critical
- GIVEN a finding with severity `Critical`, `cross_validated_by = 3`, and architecture nit text
- WHEN `apply_severity_auditor()` is called
- THEN severity stays `Critical` (multi-agent guard fires before pattern check)
- AND `severity_audit_reason` starts with `"protected_by_multi_agent_critical"`

#### Scenario: Full pipeline — architecture nit downgrade
- GIVEN a finding with severity `High`, text `"SRP violation — should be refactored"`
- WHEN `apply_severity_auditor()` is called
- THEN severity drops to `Low` (High -> -2 = Low)
- AND `severity_audited = true`
- AND `severity_audit_reason` starts with `"downgraded:"`

#### Scenario: Full pipeline — hypothetical downgrade
- GIVEN a finding with severity `High`, text `"Could cause a performance issue"`
- WHEN `apply_severity_auditor()` is called
- THEN severity drops to `Medium` (High -> -1 = Medium)

#### Scenario: Full pipeline — style nit downgrade
- GIVEN a finding with severity `High`, text `"Naming convention is inconsistent"`
- WHEN `apply_severity_auditor()` is called
- THEN severity drops to `Low` (High -> -3 = Low, clamped)

#### Scenario: No upgrade
- GIVEN a finding with severity `Low`
- WHEN `apply_severity_auditor()` is called
- THEN severity stays `Low` (never upgrades)

#### Scenario: Clamped at Low
- GIVEN a finding with severity `Medium` matching a style nit (quantum -3)
- WHEN `apply_severity_auditor()` is called
- THEN severity drops to `Low` (clamped; Medium - 3 would exceed Low)

#### Scenario: No inflated pattern
- GIVEN a finding with no inflated pattern match
- WHEN `apply_severity_auditor()` is called
- THEN severity stays unchanged
- AND `severity_audit_reason = "no_inflated_patterns_matched"`

### Requirement: Audit Report Generation
The system SHALL generate a human-readable report comparing findings before and after severity auditing.

#### Scenario: Report structure
- GIVEN lists of findings before and after auditing
- WHEN `format_severity_audit_report()` is called
- THEN it returns a multi-line string with:
  - Total findings checked
  - Count downgraded (with per-category breakdown and percentages)
  - Count protected by never-downgrade patterns
  - Count skipped by multi-agent critical guard
  - Preservation rate
  - Up to 5 sample downgrades with original/new severity and reason

