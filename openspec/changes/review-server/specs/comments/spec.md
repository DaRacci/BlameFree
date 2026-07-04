# Spec: Comment Response Format

## ADDED Requirements

### Requirement: Review Finding Structure
Each individual finding from the review pipeline SHALL be represented as a structured `ReviewFinding` object.

#### Finding schema
```json
{
    "file": "src/main.rs",
    "line": 42,
    "body": "**Unsafe unwrap usage**\n\nUsing `.unwrap()` on a `Result`...",
    "severity": "High",
    "rule_code": "SA-001",
    "suggestion": "```rust\nclient.get(url).await?;\n```",
    "source_role": "SA"
}
```

#### Field specifications
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `file` | `Option<String>` | No | Path to the changed file (relative to repo root). `None` if not applicable |
| `line` | `Option<u32>` | No | Line number (1-indexed). `None` if file-level finding |
| `body` | `String` | Yes | Markdown-formatted review comment with analysis chain |
| `severity` | `String` | Yes | One of: `"Critical"`, `"High"`, `"Medium"`, `"Low"` |
| `rule_code` | `Option<String>` | No | Code identifying the rule or agent category (e.g. `"SA-001"`, `"SEC-003"`) |
| `suggestion` | `Option<String>` | No | Optional committable fix code block in markdown |
| `source_role` | `String` | Yes | Which agent role produced this finding: `"SA"`, `"CL"`, `"AR"`, `"SEC"` |

#### Conversion from Finding
`crb-agents::Finding` -> `ReviewFinding`:
```
ReviewFinding.file         = Finding.file
ReviewFinding.line         = Finding.line
ReviewFinding.body         = format_markdown_comment(finding)  // Rich markdown with analysis
ReviewFinding.severity     = Finding.severity
ReviewFinding.rule_code    = Finding.rule_code
ReviewFinding.suggestion   = None (or extracted from finding message)
ReviewFinding.source_role  = role.as_str()
```

### Requirement: Body Markdown Format
The `body` field SHALL contain a rich markdown comment that communicates the finding's analysis chain.

#### Body structure template
```
**{title}**

{description}

**Severity:** {severity}
**Rule:** {rule_code}
**Found by:** {source_role}
```

#### Example body
```markdown
**Unsafe unwrap usage**

Using `.unwrap()` on a `Result` from a network call will panic if the request fails. Consider using the `?` operator or proper error handling.

**Severity:** High
**Rule:** SA-001
**Found by:** SA agent
```

### Requirement: GitHub-Compatible Comments
The `GET /review/{id}/comments` endpoint SHALL return findings in a format compatible with the GitHub Checks API and PR review comments endpoint.

#### Comment schema
```json
{
    "file": "src/main.rs",
    "line": 42,
    "body": "**Unsafe unwrap usage**...",
    "severity": "High",
    "rule_code": "SA-001",
    "suggestion": "```rust\nclient.get(url).await?;\n```"
}
```

#### Field mapping (ReviewFinding -> ReviewComment)
| ReviewComment field | Source | Notes |
|---------------------|--------|-------|
| `file` | `ReviewFinding.file` | Default to `""` if `None` |
| `line` | `ReviewFinding.line` | Default to `0` if `None` |
| `body` | `ReviewFinding.body` | Pass through as-is |
| `severity` | `ReviewFinding.severity` | Pass through as-is |
| `rule_code` | `ReviewFinding.rule_code` | Pass through as-is |
| `suggestion` | `ReviewFinding.suggestion` | Pass through as-is |

Note: `source_role` is excluded from the GitHub-compatible format since GitHub does not have an equivalent field.

### Requirement: Metrics Aggregation
The system SHALL compute and return aggregated metrics alongside findings.

#### Metrics schema
```json
{
    "total_findings": 5,
    "critical_count": 0,
    "high_count": 2,
    "medium_count": 2,
    "low_count": 1,
    "by_role": {
        "SA": 2,
        "CL": 1,
        "SEC": 2
    }
}
```

#### Computation rules
- `total_findings` — total number of findings across all agents
- `critical_count` — count of findings with severity == "Critical"
- `high_count` — count of findings with severity == "High"
- `medium_count` — count of findings with severity == "Medium"
- `low_count` — count of findings with severity == "Low"
- `by_role` — map of agent role -> findings count for that role

### Requirement: Severity Classification
Findings SHALL be classified into one of four severity levels.

#### Severity definitions
| Severity | Definition | Example |
|----------|------------|---------|
| **Critical** | Security vulnerability or data-loss bug that WILL cause production failure or breach | SQL injection, authentication bypass, RCE |
| **High** | Definite bug or significant code quality issue that MAY cause production failure | Null pointer dereference, race condition, deadlock |
| **Medium** | Code smell, minor correctness issue, or maintainability concern | Unused variable, overly complex function, missing error handling |
| **Low** | Style issue, minor suggestion, or documentation improvement | Naming convention violation, missing comments, formatting |

#### Default severity
If an agent's finding does not specify a severity, it SHALL default to `"Medium"`.

### Requirement: Suggestion Format
Optional suggestions SHALL be formatted as committable code blocks.

#### Suggestion structure
- Wrapped in markdown fenced code blocks with language hint
- Should be directly copy-pasteable into the file at the specified line
- May include multiple lines

#### Example suggestion
```
Fix by replacing the unwrap with proper error handling:

```rust
let response = client
    .get(url)
    .await
    .map_err(|e| AppError::Network(e))?;
```
```

### Requirement: Empty Response Handling

#### Scenario: No findings
- GIVEN a completed review with zero findings
- WHEN the client fetches the review status or comments
- THEN the findings array SHALL be empty (`[]`)
- AND the metrics SHALL show all zeros
- AND the comments endpoint SHALL return an empty array

#### Scenario: Processing error
- GIVEN a review that failed during processing
- WHEN the client fetches the review status
- THEN the status SHALL be `"failed"` with an error detail string
- AND the findings array SHALL be empty
- AND the comments endpoint SHALL return HTTP 409 Conflict

### Requirement: Backward Compatibility
The `ReviewComment` format SHALL be compatible with the existing `crb-reporting::GoldenCommentEntry` structure when used in non-server contexts.

#### Compatibility mapping
```
ReviewComment.file     -> GoldenCommentEntry file context (future field)
ReviewComment.line     -> GoldenComment line number (future field)
ReviewComment.body     -> GoldenComment.comment (when used as ground truth)
ReviewComment.severity -> GoldenComment.severity
ReviewComment.rule_code -> agent source (future field)
```

Note: The existing `GoldenCommentEntry` does not have `file`/`line` fields (they default to empty/0). The server's `ReviewComment` introduces these fields, which can be backfilled into golden comments as the dataset evolves.
