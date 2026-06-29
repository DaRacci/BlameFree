# Tasks: CRG Context Map

## Phase 0: Research & Prototyping

- [ ] **0.1 Install tree-sitter CLI + verify on discourse-graphite benchmark repos**
  - Install tree-sitter CLI
  - Verify parsing on 5 sample repos from the discourse-graphite benchmark
  - Measure parse times and output sizes
- [ ] **0.2 Python prototype: `scripts/build_context_map.py`**
  - Implement Python script using `tree-sitter` Python bindings
  - Parse 5 sample PR diffs against their repos
  - Extract definitions, references, imports per file
  - Build cross-file reference graph
  - Output JSON + compact text format
- [ ] **0.3 Validate context map against baseline**
  - Compare F1 scores: raw diff baseline vs. context map injection
  - Measure token sizes of compact text for each PR
  - Determine if context map achieves F1 ≥ 75% baseline
- [ ] **0.4 Document findings and adjust schema**
  - Report token sizes, parse times, F1 comparison
  - Adjust data schema based on gaps found during validation
  - Update estimate for Phase 1 crate implementation

## Phase 1: Context Map Builder (crb-context crate)

- [ ] **1.1 Create `crb-context` crate with Rust tree-sitter bindings**
  - `cargo init crates/crb-context`
  - Add dependencies: `tree-sitter`, `serde`, `serde_json`, `rayon`
  - Add language grammars for all 7 supported languages
  - Define `ContextMap` struct with all schema sections
- [ ] **1.2 Implement per-language extractors**
  - Python extractor (`definitions.rs`, `references.rs`)
  - JavaScript/TypeScript extractor
  - Rust extractor
  - Go extractor
  - Java extractor
  - Ruby extractor
  - Language detection by file extension
  - Graceful fallback for parse errors
- [ ] **1.3 Implement dependency graph builder** (`dependencies.rs`)
  - Extract import/require/use statements per language
  - Resolve relative imports to absolute file paths
  - Build forward and reverse dependency maps
  - Separate internal vs. external dependencies
- [ ] **1.4 Implement call graph builder** (`graph.rs`)
  - Extract function call sites per language
  - Resolve calls to defined symbols (intra-file and cross-file)
  - Store caller-callee edges with file:line precision
- [ ] **1.5 Implement diff-aware context extraction** (`diff.rs`)
  - Parse PR diff hunks
  - Map diff changes to affected symbols in CRG
  - Identify callers/callees of changed symbols
  - Build `affected_symbols` index
- [ ] **1.6 Implement compact text renderer** (`serialization.rs`)
  - Render file structure section
  - Render definitions section with signatures and docstrings
  - Render dependencies section
  - Render diff changes section
  - Budget-aware truncation with truncation footer
- [ ] **1.7 Implement PageRank ranking** (`ranking.rs`)
  - Build file dependency graph adjacency matrix
  - Run PageRank iteration (convergence threshold: 0.001)
  - Diff-aware boost: +3x for changed files, +1.5x for direct dependents
  - Store scores in `ContextMap`
- [ ] **1.8 Implement caching layer** (`cache.rs`)
  - Compute `repo_state_hash` from HEAD commit hash
  - Compute `context_cache_key` from repo_state_hash + diff_hash + model_name
  - Implement `from_cache()` and `save_to_cache()`
  - On-disk cache directory: `~/.cache/crb-context/`
- [ ] **1.9 Integration test: build context map for 10 PRs**
  - Parse 10 discourse-graphite benchmark PRs
  - Measure: build time, compact text token count, cache hit rate
  - Validate: all changed symbols appear in diff_context
  - Validate: all callers/callees of changed symbols are captured

## Phase 2: Query Tools

- [ ] **2.1 Implement `context_query` tool** (`tools.rs`)
  - Pattern-matching router for natural language queries
  - Supported patterns: "What functions/classes in [file]", "What does [symbol] do",
    "What files import from [file]", "Who calls [symbol]", "What tests cover [symbol]"
  - Return formatted output from CRG indexes
  - Return "unrecognized query" message with examples
- [ ] **2.2 Implement `read_context_section` tool**
  - Read from pre-cached file content snapshots
  - Support `start_line` and `max_lines` parameters
  - Path-safety check (canonicalization, prefix check)
  - Line cap: 200 max, truncation footer
- [ ] **2.3 Implement `find_references` tool**
  - Direct lookup in CRG `references` index
  - Optional `file` parameter for scoped search
  - Return file:line:context tuples
- [ ] **2.4 Implement `find_definition` tool**
  - Direct lookup in CRG `definitions` index
  - Handle ambiguous symbols (multiple definitions same name)
  - Return file, line, signature, docstring
- [ ] **2.5 Implement `show_diff_context` tool**
  - Read from `diff_context` section
  - Return structured summary of changed files, hunks, affected symbols
  - Return "no diff available" message when diff_context is None
- [ ] **2.6 Register tools in agent builder**
  - Create `ContextMapTool` trait and 5 implementations
  - Wire tool registration into `build_agent()` when `context_map` is Some
  - Support injection-only / tools-only / hybrid toggle
- [ ] **2.7 Update tool preamble in agent prompts**
  - Add context map preamble describing the 5 query tools
  - Specify tools read from pre-computed data (not live filesystem)
  - Include usage examples for each tool

## Phase 3: Prompt Injection

- [ ] **3.1 Add `{context_map}` template variable to prompt library**
  - Register `{context_map}` as a supported template variable
  - Resolve to `context_map.render_compact(default_budget)` when injection is enabled
  - Resolve to empty string or placeholder when disabled
- [ ] **3.2 Implement budget-aware context selection**
  - Default budget: 2000 tokens
  - Dynamic expansion: up to 4000 tokens for diffs with >10 changed files
  - Fallback modes: omit dependencies section, abbreviate file structure
  - Truncation footer message
- [ ] **3.3 Inject compact context into all 4 agent roles**
  - Static Analysis (SA) role
  - Code Logic (CL) role
  - Architecture (AR) role
  - Security (SEC) role
  - All roles get same context map (role-specific filtering is future work)
- [ ] **3.4 Implement hybrid mode option**
  - `context_map_mode = "injection_only"` — inject only, no query tools
  - `context_map_mode = "tools_only"` — query tools only, no injection
  - `context_map_mode = "hybrid"` — both injection and query tools (default)
  - `context_map_mode = "disabled"` — neither (legacy live tools)
- [ ] **3.5 Configuration toggle**
  - Add `context_map_mode` to config file
  - Add `--context-map-mode` CLI flag override
  - Add `use_context_injection` parameter to `build_agent()`

## Phase 4: Integration & Benchmarking

- [ ] **4.1 Wire context map build into consensus pipeline**
  - Update `evaluate_pr_with_consensus()` to build context map before running reviewers
  - Build context map from `repo_path` and `diff` before agent creation
  - Pass context map to `build_agent()` for each role
- [ ] **4.2 Integrate with existing cache layer**
  - Use existing `gatherer_prompt_hash` and `diff_hash` from consensus pipeline
  - Compute `repo_state_hash` from HEAD
  - Store context map in cache keyed by `(gatherer_prompt_hash, diff_hash, repo_state_hash)`
  - Check cache before rebuilding
- [ ] **4.3 Run benchmark comparison**
  - **Baseline**: raw diff only (current ~75% F1) — 4 agents, no tools
  - **Context map injection only**: no tools, just injected context
  - **Hybrid**: injection + 5 query tools, max 2 turns
  - **Live tools**: original read_file/grep/terminal approach (8x tokens)
  - Benchmark against 20 discourse-graphite PRs
- [ ] **4.4 Measure results**
  - F1 score for each approach
  - Token cost per PR (input + output)
  - Latency (wall-clock time per PR)
  - Cache hit rate
  - Context map build time
  - Frequency of query tool usage (hybrid mode)
- [ ] **4.5 Iterate on context map design**
  - If F1 < 75% for injection-only: increase budget, improve ranking, add more symbol detail
  - If query tools underused: improve tool descriptions, add more useful queries
  - If build time > 10s for <2000 file repos: optimize parsing, increase parallelism
  - If cache hit rate < 80%: investigate cache key collisions, increase TTL

## Phase 5: Documentation & Polish

- [ ] **5.1 Update architecture docs** — Document the CRG context map pipeline in project docs
- [ ] **5.2 API docs** — Module-level and function-level `///` docs in `crb-context` crate
- [ ] **5.3 Developer guide** — How to add a new language, how to add a new query tool
- [ ] **5.4 Benchmark results** — Publish benchmark results comparing all 4 approaches
