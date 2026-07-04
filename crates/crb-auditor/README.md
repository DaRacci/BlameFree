# crb-auditor

Rule-based severity auditor that detects inflated severity labels in code review findings and applies downgrades with an audit trail.

- **downgrade patterns** across 3 categories: `architecture_nits` (−2), `hypothetical_theoretical` (−1), `style_nits` (−3)
- **never-downgrade patterns** protecting genuine security vulnerabilities, data-integrity issues, and correctness bugs
- Multi-agent critical findings (≥2 agents flagging CRITICAL) are also protected from downgrade
