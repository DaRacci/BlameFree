# logs Specification

## Purpose
Log viewing backend endpoints and cache resolution for serving agent log data from completed benchmark runs.
## Requirements
### Requirement: List Logs Endpoint

The system SHALL expose `GET /api/runs/:id/logs` to list all available PR log entries for a completed run, merging PRs from the output directory (canonical source) with cache-only PRs.

#### Scenario: List logs with cache available

- GIVEN a run `run-123` with output JSON files for PRs `pr-fix-1` and `pr-fix-2`, and a cache directory containing agent folders for both PRs
- WHEN `GET /api/runs/:id/logs` is called
- THEN it returns 200 with `cache_available = true`
- AND `prs` contains 2 entries
- AND each entry has `pr_key`, `pr_title` (from JSON content), and `agents` array with role info (abbreviation + name)

#### Scenario: List logs with cache unavailable

- GIVEN a run `run-123` with output JSON files but no cache directory exists
- WHEN `GET /api/runs/:id/logs` is called
- THEN it returns 200 with `cache_available = false`
- AND `prs` contains entries from the output directory with empty `agents` arrays

#### Scenario: List logs returns 404 for unknown run

- GIVEN a run ID that has no output directory
- WHEN `GET /api/runs/:id/logs` is called
- THEN it returns a 404 error response

#### Scenario: Cache-only PRs included in list

- GIVEN a run with output directory containing PR `pr-a` and a cache directory with agents for PR `pr-a` and PR `pr-b` (cache-only)
- WHEN `GET /api/runs/:id/logs` is called
- THEN `prs` includes both `pr-a` (with title from output JSON) and `pr-b` (with title resolved from cache)
- AND `pr-a` has agents scanned from cache
- AND `pr-b` has agents scanned from cache

#### Scenario: Output directory PRs are canonical source

- GIVEN a run where the cache directory references a PR key that also exists in the output directory
- WHEN `GET /api/runs/:id/logs` is called
- THEN the PR entry uses the title from the output JSON (not the cache fallback)
- AND duplicate keys are merged into a single entry

#### Scenario: Summary and candidates files are excluded

- GIVEN a run output directory containing `_summary.json`, `candidates_123.json`, and a normal PR file `42.json`
- WHEN `GET /api/runs/:id/logs` is called
- THEN only the normal PR file `42.json` is included in the response

---

### Requirement: Agent Log Endpoint

The system SHALL expose `GET /api/runs/:id/logs/:pr_key/:role` to retrieve the prompt, response, and reasoning text for a specific agent role within a PR.

#### Scenario: Fetch agent log with all content

- GIVEN a cache directory for run `run-123` with PR `my-pr` containing content-addressed files for role SA:
  - `<hash>.agent_SA_prompt.txt`
  - `<hash>.agent_SA_response.txt`
  - `<hash>.agent_SA_reasoning.txt`
- WHEN `GET /api/runs/:id/logs/my-pr/SA` is called
- THEN it returns 200 with JSON containing:
  - `prompt`: the decoded prompt file content
  - `response`: the decoded response file content
  - `reasoning`: the decoded reasoning file content
  - `available: true`

#### Scenario: Fetch agent log with partial content

- GIVEN a cache directory for PR `my-pr` with only `agent_CL_prompt.txt` and `agent_CL_response.txt` (no reasoning file) in simple layout
- WHEN `GET /api/runs/:id/logs/my-pr/CL` is called
- THEN it returns 200 with `prompt` and `response` populated
- AND `reasoning` is `null`
- AND `available: true`

#### Scenario: Fetch agent log with no cache available

- GIVEN a run `run-123` with no cache directory
- WHEN `GET /api/runs/:id/logs/my-pr/SA` is called
- THEN it returns 200 with `available: false`
- AND `prompt`, `response`, `reasoning` are all `null`

#### Scenario: Fetch agent log for non-existent PR

- GIVEN a cache directory exists for run `run-123` but does NOT have a subdirectory for PR `nonexistent-pr`
- WHEN `GET /api/runs/:id/logs/nonexistent-pr/SA` is called
- THEN it returns 200 with `available: false` and all content fields `null`

#### Scenario: UTF-8 lossy decoding for malformed content

- GIVEN a cache file containing invalid UTF-8 bytes
- WHEN `GET /api/runs/:id/logs/:pr_key/:role` is called
- THEN the content is decoded using `String::from_utf8_lossy`
- AND invalid byte sequences are replaced with the Unicode replacement character

#### Scenario: Content-addressed file discovery

- GIVEN a PR cache directory with `agents/` subdirectory containing files like `abc123.agent_SA_prompt.txt` and `def456.agent_SA_response.txt`
- WHEN `read_agent_log_file()` searches for role SA, suffix "prompt"
- THEN it matches any file ending with `.agent_SA_prompt.txt` in the `agents/` subdirectory
- AND returns the content regardless of the hash prefix

#### Scenario: Simple layout file discovery (fallback)

- GIVEN a PR cache directory containing `agent_SA_prompt.txt` directly (no `agents/` subdirectory)
- WHEN `read_agent_log_file()` searches for role SA, suffix "prompt"
- THEN it finds the file at `pr_key/agent_SA_prompt.txt`
- AND returns the decoded content

---

### Requirement: PR Agent Availability Endpoint

The system SHALL expose `GET /api/runs/:id/prs/:pr_key` to return agent availability metadata for a single PR, including which agents have cached log files and their prompt/response/reasoning status.

#### Scenario: Get PR with multiple agents

- GIVEN a cache directory with agents SA (prompt+response+reasoning) and CL (prompt+response only)
- WHEN `GET /api/runs/:id/prs/:pr_key` is called
- THEN it returns `PrAgentsResponse` with `pr_title` resolved from output JSON
- AND `agents` contains 2 entries:
  - SA: `has_prompt: true, has_response: true, has_reasoning: true`
  - CL: `has_prompt: true, has_response: true, has_reasoning: false`
- AND `has_output: true` (output JSON file exists)

#### Scenario: Get PR with no cache

- GIVEN a run with no cache directory for this PR
- WHEN `GET /api/runs/:id/prs/:pr_key` is called
- THEN it returns with `agents: []` and `has_output: false`
- AND the PR title is still resolved from the output JSON if available

#### Scenario: Agent role scanning from content-addressed layout

- GIVEN a PR cache directory with `agents/` containing:
  - `<hash1>.agent_SA_prompt.txt`
  - `<hash2>.agent_SA_response.txt`
  - `<hash3>.agent_CL_prompt.txt`
- WHEN `scan_agent_roles()` is called
- THEN it returns roles `["CL", "SA"]` (sorted alphabetically)
- AND duplicates are deduplicated (same role across prompt/response files yields one entry)

#### Scenario: Agent role scanning from simple layout

- GIVEN a PR cache directory containing:
  - `agent_SA_prompt.txt`
  - `agent_SA_response.txt`
  - `agent_CL_prompt.txt`
  - `agent_CL_response.txt`
- WHEN `scan_agent_roles()` is called
- THEN it returns roles `["CL", "SA"]`
- AND the role entries have `abbreviation` and `name` both set to the role string

#### Scenario: has_output flag from directory scan

- GIVEN a PR `fix-bug` whose output JSON file `fix-bug.json` exists in the run directory
- WHEN `GET /api/runs/:id/prs/fix-bug` is called
- THEN `has_output` is `true`

---

### Requirement: Log Response Types in Shared Crate

The system SHALL define all log-related request/response types in the `crb-webui-shared` crate's `runs.rs` module, shared between backend and frontend.

#### Scenario: LogsListResponse type

- GIVEN a `LogsListResponse` struct with fields `run_id: String`, `cache_available: bool`, `prs: Vec<PrLogsEntry>`
- WHEN the struct is serialized to JSON and deserialized back
- THEN the round-trip preserves all field values
- AND it derives `Serialize` and `Deserialize`

#### Scenario: AgentLogResponse type

- GIVEN an `AgentLogResponse` struct with fields `run_id`, `pr_key`, `role`, `prompt: Option<String>`, `response: Option<String>`, `reasoning: Option<String>`, `available: bool`
- WHEN the struct is serialized with some optional fields set to `None`
- THEN those fields appear as `null` in JSON
- AND the round-trip preserves all values

#### Scenario: PrAgentsResponse type

- GIVEN a `PrAgentsResponse` struct with fields `run_id`, `pr_key`, `pr_title`, `agents: Vec<PrAgentEntry>`, `has_output: bool`
- WHEN the struct is serialized and deserialized
- THEN all fields round-trip correctly

#### Scenario: PrAgentEntry type

- GIVEN a `PrAgentEntry` struct with fields `role: String`, `has_prompt: bool`, `has_response: bool`, `has_reasoning: bool`
- WHEN the struct is serialized to JSON
- THEN it produces `{"role": "...", "has_prompt": bool, "has_response": bool, "has_reasoning": bool}`

---

### Requirement: Cache Directory Resolution

The system SHALL resolve the cache directory by trying multiple layouts, supporting both current and legacy directory structures.

#### Scenario: New layout (output/_cache/)

- GIVEN an output directory at `output/run-123/` with a sibling `output/_cache/` directory
- WHEN `resolve_cache_dir()` is called
- THEN it returns `output/_cache/`
- AND the `_cache` directory name is defined by `crb_cache::paths::CACHE_DIR_NAME`

#### Scenario: Legacy layouts checked as fallback

- GIVEN that `output/_cache/` does not exist
- WHEN `resolve_cache_dir()` is called
- THEN it tries fallbacks in order:
  1. `output/run-123/cache/`
  2. `output/cache/run-123/`
  3. `output/cache/`
- AND returns the first one that exists

#### Scenario: No cache directory found

- GIVEN none of the candidate cache directories exist
- WHEN `resolve_cache_dir()` is called
- THEN it returns `None`

---

