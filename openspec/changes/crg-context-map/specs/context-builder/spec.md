# Delta for Context Map Builder

> **Note (2026-07-16):** This spec targets a hypothetical `crb-context` crate that does not exist in the codebase. Not yet implemented.

## ADDED Requirements

### Requirement: Build Context Map from Repository

The system SHALL build a Code-Reference Graph (CRG) context map from a repository worktree by parsing all source files with tree-sitter and extracting symbols, definitions, references, and dependencies.

#### Scenario: Full Build of Small Repository

GIVEN a repository with 50-100 source files in supported languages
WHEN `ContextMap::build(&repo_path, None)` is called
THEN the system SHALL parse all source files
AND extract all function/class/method/variable definitions
AND extract all import/require statements
AND extract all function call references
AND return a `ContextMap` struct with populated `file_tree`, `symbols`, and `dependencies`

#### Scenario: Build with Diff Context

GIVEN a repository path and a PR diff string
WHEN `ContextMap::build(&repo_path, Some(&diff))` is called
THEN the system SHALL parse all source files
AND compute the `diff_context` section showing changed files, hunks, and affected symbols
AND mark changed symbols in the `symbols` index with `changed: true`

### Requirement: Per-Language Extractors

The system SHALL implement per-language extractors for Python, JavaScript, TypeScript, Rust, Go, Java, and Ruby using tree-sitter grammars.

#### Scenario: Python Definition Extraction

GIVEN a Python file with function definitions, class definitions, and import statements
WHEN the Python extractor processes the file
THEN the system SHALL extract `function_definition` nodes as function symbols
AND extract `class_definition` nodes as class symbols
AND extract `import_statement` nodes as dependency edges

#### Scenario: JavaScript Definition Extraction

GIVEN a JavaScript file with function declarations, class declarations, and require/import statements
WHEN the JavaScript extractor processes the file
THEN the system SHALL extract `function_declaration` and `arrow_function` nodes as function symbols
AND extract `class_declaration` nodes as class symbols
AND extract `import_statement` and `call_expression`(require) nodes as dependency edges

#### Scenario: Rust Definition Extraction

GIVEN a Rust file with `fn` declarations, `struct` definitions, and `use` statements
WHEN the Rust extractor processes the file
THEN the system SHALL extract `function_item` nodes as function symbols
AND extract `struct_item` nodes as struct symbols
AND extract `use_declaration` nodes as dependency edges

#### Scenario: Unsupported Language

GIVEN a source file whose extension does not match any supported language
WHEN the system processes the file
THEN the system SHALL skip the file
AND log a warning with the file path and detected extension

#### Scenario: Parse Error Graceful Fallback

GIVEN a source file with syntax errors that causes tree-sitter to return a partial AST
WHEN the system parses the file
THEN the system SHALL proceed with the partial AST (extract what is parseable)
AND log a warning with the file path and error count

### Requirement: Dependency Graph Builder

The system SHALL build a cross-file dependency graph from extracted import/require/use statements.

#### Scenario: File-to-File Dependency Resolution

GIVEN a file `src/auth.py` that imports `src.config` and `src.models.user`
WHEN the dependency graph builder resolves imports
THEN the system SHALL add an edge from `src/auth.py` to each resolved path
AND populate the `imported_by` reverse index for each target file

#### Scenario: External Dependency Tracking

GIVEN a file that imports external packages (e.g., `os`, `jwt`, `requests`)
WHEN the dependency graph builder processes imports
THEN the system SHALL record external dependencies separately from internal file paths
AND include them in the `dependencies.imports` list marked as external

### Requirement: Call Graph Builder

The system SHALL build a call graph by extracting function call sites and resolving them to defined symbols.

#### Scenario: Intra-File Call Extraction

GIVEN a Python file containing `validate_token(...)` called within `login_handler`
WHEN the system extracts the call graph
THEN the system SHALL record a `call_graph` entry with:
- `caller`: the containing function (`login_handler`, file, line)
- `callee`: the called function (`validate_token`, file, line)
- `call_site`: the exact line of the call

#### Scenario: Cross-File Call Resolution

GIVEN `src/api/handlers.py` imports and calls `validate_token` from `src/auth.py`
WHEN the system resolves cross-file calls
THEN the system SHALL resolve the reference to the definition in `src/auth.py`
AND record the cross-file call graph edge

### Requirement: Compact Text Renderer

The system SHALL render the context map into a compact text format suitable for direct injection into LLM prompts.

#### Scenario: Render Default Format

GIVEN a populated `ContextMap` with file tree, definitions, dependencies, and diff context
WHEN `context_map.render_compact(token_budget)` is called with a budget of 2000 tokens
THEN the system SHALL produce text with sections: `=== FILE STRUCTURE ===`, `=== DEFINITIONS ===`, `=== DEPENDENCIES ===`, `=== DIFF CHANGES ===`
AND each section SHALL be populated with the corresponding data
AND the total rendered text SHALL not exceed the specified token budget

#### Scenario: Render with Token Truncation

GIVEN a large repository where the full compact text exceeds the token budget
WHEN `context_map.render_compact(token_budget)` is called
THEN the system SHALL apply PageRank-based ranking to select the most relevant definitions
AND prioritize diff-affected symbols
AND truncate low-priority sections to fit within the budget
AND append a footer indicating truncation occurred

### Requirement: Caching

The system SHALL cache context maps using a content-addressed key to avoid redundant rebuilds.

#### Scenario: Cache Hit

GIVEN a cache key `sha256(HEAD + diff_hash + model)` that matches an existing cache entry
WHEN `ContextMap::from_cache(&key)` is called
THEN the system SHALL return the cached `ContextMap` without re-parsing any files

#### Scenario: Cache Miss

GIVEN a cache key with no matching cache entry
WHEN `ContextMap::from_cache(&key)` is called
THEN the system SHALL return `None`
AND the caller SHALL proceed to build the context map and call `save_to_cache`

#### Scenario: Cache Invalidation

GIVEN the repository HEAD changes
WHEN the cache key is recomputed
THEN the system SHALL produce a different cache key
AND the previous cache entry SHALL be invalidated (stale but not deleted; overwritten on next build)
