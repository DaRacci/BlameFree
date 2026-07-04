# Design: CRG Context Map for Code Review

## 1. Architecture Overview

The CRG context map system has four layers:

```
┌─────────────────────────────────────────────────────────────┐
│                     CRG Builder (crb-context)                │
│                                                             │
│  tree-sitter ─► per-language extractors ─► CRG (graph)     │
│       ▲                                                      │
│  source files                                                 │
│       ▲                                                      │
│  worktree clone                                               │
└─────────────────────────┬───────────────────────────────────┘
                          │ render_compact()
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                Compact Text Renderer                         │
│  "=== FILE STRUCTURE ===\nsrc/auth.py (python, 1523b)\n..."  │
└─────────────────────────┬───────────────────────────────────┘
                          │ injected as {context_map}
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    Agent System Prompt                        │
│  [role preamble] + [context_map] + [rules] + [diff]          │
│                    │                                          │
│                    ▼                                          │
│               Query Tools (read-only)                         │
│  context_query │ read_context_section │ find_references      │
│  find_definition │ show_diff_context                          │
└─────────────────────────────────────────────────────────────┘
```

### Integration with Existing Pipeline

**Current pipeline (from `crb-consensus`):**
```
diff -> build_agent() -> agent.prompt(diff) -> parse findings -> judge
                          ↑
                  tools (read_file, grep, terminal, list_dir)
```

**New pipeline:**
```
diff -> build_context_map(repo_path)
     -> context_map.render_compact()  ← injected as {context_map} template var
     -> build_agent(context_map_injected)
     -> agent.prompt(diff)             ← no tool calls needed (usually)
     -> parse findings -> judge
                          ↑
                  query tools (context_query, read_context_section,
                               find_references, find_definition,
                               show_diff_context)
```

## 2. Data Schema

The context map is a single JSON document (or SQLite database for large repos) containing six sections:

### 2.1 File Structure Tree

```json
{
  "file_tree": {
    "root": "repo_name",
    "children": [
      { "path": "src/main.py", "size": 1523, "lang": "python" },
      { "path": "src/utils.py", "size": 890, "lang": "python" }
    ]
  }
}
```

### 2.2 Symbol Definitions

```json
{
  "symbols": {
    "definitions": [
      {
        "name": "validate_token",
        "kind": "function",
        "file": "src/auth.py",
        "line": 42,
        "signature": "def validate_token(token: str, user_id: int) -> bool",
        "docstring": "Validate JWT token for user",
        "exported": true
      }
    ],
    "references": [
      {
        "name": "validate_token",
        "file": "src/api/handlers.py",
        "line": 55,
        "context": "result = validate_token(request.token, request.user_id)"
      }
    ]
  }
}
```

### 2.3 Import/Dependency Graph

```json
{
  "dependencies": {
    "src/auth.py": {
      "imports": ["os", "jwt", "src.config", "src.models.user"],
      "imported_by": ["src/api/handlers.py", "src/middleware.py"]
    }
  }
}
```

### 2.4 Call Graph

```json
{
  "call_graph": [
    {
      "caller": { "file": "src/api/handlers.py", "function": "login_handler", "line": 30 },
      "callee": { "file": "src/auth.py", "function": "validate_token", "line": 42 },
      "call_site": { "file": "src/api/handlers.py", "line": 55 }
    }
  ]
}
```

### 2.5 Diff Context

```json
{
  "diff_context": {
    "changed_files": ["src/auth.py", "src/api/handlers.py"],
    "changes": [
      {
        "file": "src/auth.py", "type": "modified",
        "additions": 15, "deletions": 3,
        "hunks": [
          { "start_line": 42, "end_line": 58,
            "added": ["def validate_token(...)", ...], "removed": [] }
        ]
      }
    ],
    "affected_symbols": [
      { "name": "validate_token", "kind": "function", "file": "src/auth.py",
        "changed": true, "callers": ["login_handler"] }
    ]
  }
}
```

### 2.6 Compact Text Format (Prompt Injection)

```text
=== FILE STRUCTURE ===
src/auth.py (python, 1523b)
src/api/handlers.py (python, 2045b)

=== DEFINITIONS ===
func validate_token(src/auth.py:42) -> bool
  doc: Validate JWT token for user
  callers: login_handler(src/api/handlers.py:30)
class AuthService(src/auth.py:100) extends BaseService
  methods: login, logout, refresh_token

=== DEPENDENCIES ===
src/auth.py -> [os, jwt, src.config, src.models.user]

=== DIFF CHANGES ===
MODIFIED: src/auth.py (+15/-3)
  HUNK @42-58: validate_token signature changed
AFFECTED: validate_token (src/auth.py:42) — 2 callers may be affected
```

### 2.7 Test Coverage Mapping

```json
{
  "test_coverage": {
    "src/auth.py": {
      "tests": ["tests/test_auth.py::test_validate_token_valid",
                "tests/test_auth.py::test_validate_token_expired"]
    }
  }
}
```

## 3. Rust Crate Structure

### `crb-context` Crate

```
crates/crb-context/
├── Cargo.toml
└── src/
    ├── lib.rs              # Public API: build_context_map(), ContextMap struct
    ├── parser.rs           # tree-sitter parsing orchestration (parallel via rayon)
    ├── definitions.rs      # Per-language extractors for symbol definitions
    ├── references.rs       # Per-language extractors for symbol references/calls
    ├── dependencies.rs     # Import/dependency graph builder
    ├── diff.rs             # Diff-aware context extraction
    ├── graph.rs            # CRG data structures and traversal
    ├── ranking.rs          # PageRank-based context ranking
    ├── serialization.rs    # JSON + compact text output formats
    ├── tools.rs            # Query tool definitions (ContextMapTool implementations)
    └── cache.rs            # Content-addressed caching logic
```

### Key Dependencies

| Dependency | Purpose |
|---|---|
| `tree-sitter` | Rust bindings for tree-sitter parsing |
| `tree-sitter-python` | Python grammar |
| `tree-sitter-javascript` | JavaScript grammar |
| `tree-sitter-typescript` | TypeScript grammar |
| `tree-sitter-rust` | Rust grammar |
| `tree-sitter-go` | Go grammar |
| `tree-sitter-java` | Java grammar |
| `tree-sitter-ruby` | Ruby grammar |
| `serde` / `serde_json` | Serialization |
| `rayon` | Parallel file parsing |

### Build Time Estimates

| Repo Size | Files | Parse Time | Graph Build | Total |
|---|---|---|---|---|
| Small (<100 files) | 50-100 | <0.5s | <0.5s | ~1s |
| Medium (100-1000) | 500 | ~2s | ~1s | ~3s |
| Large (1000-5000) | 2000 | ~8s | ~4s | ~12s |
| Very Large (5000+) | 10000 | ~30s | ~15s | ~45s |

*Measured on single core with Rust; parallel parsing via rayon ≈ 4x faster.*

### Storage Format Comparison

| Format | Size (medium repo) | Query Speed | LLM Injection Friendly |
|---|---|---|---|
| JSON (full graph) | ~2-5 MB | Fast (indexed) | No — needs summarization |
| JSON (compacted) | ~500 KB-1 MB | Fast | Partial |
| SQLite | ~1-2 MB | Very fast (SQL queries) | No — needs extraction |
| **Compact Text** | **~50-100 KB** | N/A (pre-formatted) | **Yes — directly injectable** |
| MessagePack | ~500 KB-1 MB | Fast | No |

**Recommendation:** Store full graph in JSON for tool queries, pre-render compact text for prompt injection. Use SQLite if we need complex lookups.

## 4. Context Map Generation Flow

```
1. Clone/reuse worktree checkout
2. List all source files (respect .gitignore)
3. For each file:
   a. Detect language (by extension)
   b. Parse with tree-sitter
   c. Extract definitions + references + imports
4. Build cross-file reference graph
5. Compute dependency edges
6. Run PageRank on file graph (optional, for ranking)
7. Compute diff context (if diff provided)
8. Serialize to JSON + compact text format
9. Cache the result
```

### Context Map Builder API

```rust
// In crates/crb-context/src/lib.rs

pub struct ContextMap {
    // Full graph stored in JSON for tool queries
    pub file_tree: FileTree,
    pub symbols: SymbolIndex,
    pub dependencies: DependencyGraph,
    pub call_graph: CallGraph,
    pub diff_context: Option<DiffContext>,
    pub test_coverage: Option<TestCoverage>,
    // Pre-cached file content snapshots
    pub file_snapshots: HashMap<PathBuf, Vec<String>>,
}

impl ContextMap {
    /// Build context map from a repo path and optional diff.
    pub async fn build(repo_path: &Path, diff: Option<&str>) -> Result<Self>;

    /// Render compact text format for prompt injection.
    /// Accepts token budget and returns ranked, budgeted text.
    pub fn render_compact(&self, token_budget: usize) -> String;

    /// Render full JSON for tool query backend.
    pub fn to_json(&self) -> String;

    /// Load from cache (content-addressed).
    pub fn from_cache(key: &str) -> Option<Self>;

    /// Save to cache.
    pub fn save_to_cache(&self, key: &str);
}
```

## 5. Query Tool Definitions

Each tool is a `ContextMapTool` struct that implements a common trait and is registered on the agent.

### Tool 1: `context_query`

- **Description**: Natural language query over the pre-computed CRG. Routes questions like "What functions are in src/auth.py?" or "Who calls validate_token?" to deterministic CRG lookups.
- **Implementation**: Pattern matching + keyword extraction (no LLM call for routing).
- **Params**: `{ "question": "string" }`
- **Returns**: Formatted text from CRG sections.

### Tool 2: `read_context_section`

- **Description**: Reads a specific section of a file from the pre-cached content snapshot (not live filesystem).
- **Params**: `{ "path": "string", "start_line?": "uint", "max_lines?": "uint" }`
- **Returns**: File content lines within range, with truncation footer.

### Tool 3: `find_references`

- **Description**: Find all references to a symbol across the codebase.
- **Params**: `{ "symbol": "string", "file?": "string" }`
- **Returns**: File:line:context for each reference.

### Tool 4: `find_definition`

- **Description**: Find where a symbol is defined.
- **Params**: `{ "symbol": "string" }`
- **Returns**: File, line, and full definition signature.

### Tool 5: `show_diff_context`

- **Description**: Shows what the current PR diff changed and affected symbols.
- **Params**: None (uses pre-computed diff context).
- **Returns**: Structured summary of file changes and ripple effects.

### Tool Registration

```rust
// In crb-agents or crb-context tools registration
let context_map = ContextMap::from_cache("repo_hash_diff_hash")?;

client
    .agent(model)
    .preamble(&full_preamble)         // includes compact text via {context_map}
    .tool(context_map.context_query_tool())
    .tool(context_map.read_section_tool())
    .tool(context_map.find_references_tool())
    .tool(context_map.find_definition_tool())
    .tool(context_map.show_diff_context_tool())
    .temperature(0.3)
    .build()
```

## 6. Token Budget & Ranking

- **Default budget**: 2K tokens for compact text context map.
- **Dynamic expansion**: Up to 4K tokens for complex repo changes.
- **Ranking algorithm**: PageRank on file dependency graph.
- **Diff-first priority**:
  1. Files touched by the PR (highest priority)
  2. Direct dependents of changed files (importers/importees)
  3. Callers/callees of changed symbols
  4. High-PageRank files (globally important modules)
  5. Test files covering changed code

### Comparison to Aider's Approach

| Aspect | Aider (general-purpose) | Ours (code review) |
|---|---|---|
| Ranking | PageRank on file graph | PageRank + diff-aware boost |
| Test awareness | None | Include test files for changed code |
| Symbol detail | Definition line only | Signature + docstring |
| Context scope | All files | Changed files + dependents + high-rank files |
| Token budget | 1K default | 2K default (expandable to 4K) |

## 7. Cache Integration

Context map is content-addressed to avoid rebuilding on unchanged state:

```rust
let repo_state_hash = sha256_hex(&std::fs::read_to_string("HEAD")?);
let context_cache_key = compute_context_cache_key(
    &gatherer_prompt_hash, &diff_hash, &repo_state_hash, &model_name
);
```

- **Same diff + same repo state -> same context map -> cache hit.**
- **Cache TTL**: Until HEAD changes.
- **Storage**: On-disk JSON cache + serialized compact text.
- **Cache hit rate target**: ≥ 80% for repeated runs on same repo state.

## 8. Integration Points

### `crb-agents/src/lib.rs`

```rust
// New parameter: context_map_text
pub fn build_agent(
    client: &openai::Client,
    model: &str,
    role: &str,
    rules_preamble: Option<&str>,
    prompt_lib: Option<&PromptLibrary>,
    template_vars: Option<&HashMap<&str, &str>>,
    extra_preamble: Option<&str>,
    workdir: Option<&str>,
    // NEW:
    context_map: Option<&ContextMap>,
    use_context_injection: bool,
) -> Agent<ResponsesCompletionModel>
```

### `crb-consensus/src/lib.rs`

```rust
// Before running reviewers:
let context_map = ContextMapBuilder::new(&repo_path, &diff)
    .build_async()
    .await?;

// Inject context map into template vars
let mut template_vars = HashMap::new();
template_vars.insert("context_map", context_map.render_compact(2000));
```

## 9. Risk Assessment & Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Context map too large for prompt | High | Budget-aware ranking (PageRank), dynamic truncation |
| Tree-sitter parser errors on invalid syntax | Medium | Graceful fallback — skip file, log warning |
| Cross-file references wrong in dynamic languages | Medium | Conservative: only track explicit imports/references |
| Build time too long for large repos | Medium | Parallel parsing (rayon), incremental rebuild |
| Context map doesn't improve F1 | High | A/B test vs. baseline; if no improvement, use hybrid mode |
| DeepSeek V4 Flash ignores context map | Low | Already tested with prompt injection — models respect well-formatted context |
| Memory usage (large repos) | Low | Stream parsing, SQLite backend for very large repos |
