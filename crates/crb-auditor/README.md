# crb-auditor

Rule-based severity auditor — detects inflated severity labels in code review findings and applies downgrades with an audit trail. Port of the original Python `severity_auditor.py`.

- **83 downgrade patterns** across 3 categories: `architecture_nits` (−2), `hypothetical_theoretical` (−1), `style_nits` (−3)
- **40 never-downgrade patterns** protecting genuine security vulnerabilities, data-integrity issues, and correctness bugs
- Multi-agent critical findings (≥2 agents flagging CRITICAL) are also protected from downgrade

## Key types

- [`InflatedCategory`](src/lib.rs) — Downgrade rule category with name, patterns, description, and downgrade quantum
- [`apply_severity_auditor()`](src/lib.rs) — Core function: checks never-downgrade patterns, multi-agent protection, then inflated patterns; adds `severity_audited` and `severity_audit_reason` fields
- [`format_severity_audit_report()`](src/lib.rs) — Generates a human-readable before/after comparison report

## CLI usage

```bash
# Audit findings from a JSON file, output to stdout
cargo run -p crb-auditor -- --findings findings.json

# Write audited findings to a file and generate an audit report
cargo run -p crb-auditor -- \
  --findings findings.json \
  --output audited_findings.json \
  --report audit_report.txt
```
