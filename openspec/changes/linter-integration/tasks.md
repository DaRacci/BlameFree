# Tasks: Linter Integration (Rust + rig Tool Trait)

## Core Infrastructure
- [ ] Define `Finding` struct in `findings.rs` with serde `Serialize`/`Deserialize` + `schemars::JsonSchema` derives
- [ ] Create `LinterConfig` / `LinterToolConfig` structs (deserialized from TOML) in `config.rs`
- [ ] Implement TOML deserialization for linter config (language → `Vec<LinterToolConfig>`)
- [ ] Scaffold `linters.rs` module: `LinterTool` struct, `run_linters()` helper, parser dispatch

## Linter Toolkit Tool Implementations
- [ ] Implement `RuffLinter` / `LinterTool` with `parser_kind = "ruff"`: `Tool` trait, `tokio::process::Command` for `ruff check --output-format json`, serde_json parse
- [ ] Implement `EslintLinter` / `parser_kind = "eslint"`: same pattern for `eslint --format json`
- [ ] Implement `GoVetLinter` / `parser_kind = "govet"`: same pattern for `go vet`
- [ ] Implement `StaticcheckLinter` / `parser_kind = "staticcheck"`: same pattern for `staticcheck`
- [ ] Implement `RubocopLinter` / `parser_kind = "rubocop"`: same pattern for `rubocop --format json`
- [ ] Implement `CheckstyleLinter` / `parser_kind = "checkstyle"`: same pattern for `java -jar checkstyle.jar`

## Harness Integration
- [ ] Wire linter Tool calls into `evaluate_pr()` via `tokio::task::JoinSet` alongside LLM agent calls
- [ ] Tag each finding with source type (`"linter:{name}"`) for downstream filtering
- [ ] Add `--skip-linters` CLI flag to disable linter execution
- [ ] Add `--linters-only` CLI flag to run linters without LLM agents
- [ ] Add `--linters-config <path>` CLI flag to specify custom TOML config path

## Testing
- [ ] Test ruff linter against Python PRs from sentry dataset (3 PRs)
- [ ] Test eslint linter against TypeScript PRs from cal.com dataset (3 PRs)
- [ ] Test go vet + staticcheck against Go PRs from grafana dataset (3 PRs)
- [ ] Test rubocop linter against Ruby PRs (3 PRs)
- [ ] Test checkstyle linter against Java PRs (3 PRs)
- [ ] Validate that `Tool::definition()` produces valid JSON Schema for each linter
- [ ] Validate that linter findings integrate correctly into judge pipeline
- [ ] Validate timeout handling: kill linter subprocess if it exceeds `timeout_secs`
