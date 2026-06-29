# Delta for Context Query Tools

## ADDED Requirements

### Requirement: `context_query` Tool

The system SHALL provide a `context_query` tool that answers natural language questions about the repository by querying the pre-computed CRG.

#### Scenario: Query Function Definitions in a File

GIVEN a `ContextMap` with symbol definitions for `src/auth.py`
WHEN an agent calls `context_query` with question "What functions are defined in src/auth.py?"
THEN the system SHALL extract the file path from the question
AND return a formatted list of function definitions in that file with signatures and line numbers

#### Scenario: Query Symbol Purpose

GIVEN a `ContextMap` with a definition for `validate_token` including its docstring
WHEN an agent calls `context_query` with question "What does validate_token do?"
THEN the system SHALL look up `validate_token` in the definitions index
AND return its signature, docstring, file, line, and caller count

#### Scenario: Query File Dependents

GIVEN a `ContextMap` with a dependency graph entry for `src/auth.py`
WHEN an agent calls `context_query` with question "What files import from src/auth.py?"
THEN the system SHALL look up `src/auth.py` in the dependency graph
AND return the list of files that import from it

#### Scenario: Query Call Graph

GIVEN a `ContextMap` with a call graph containing entries for `login_handler`
WHEN an agent calls `context_query` with question "Show me the call graph for the login handler"
THEN the system SHALL look up `login_handler` in the call graph
AND return its callers and callees with file:line references

#### Scenario: Query Diff Impact

GIVEN a `ContextMap` with a populated `diff_context`
WHEN an agent calls `context_query` with question "What symbols were affected by the diff?"
THEN the system SHALL return the list of changed symbols and their affected callers

#### Scenario: Query Test Coverage

GIVEN a `ContextMap` with test coverage mapping
WHEN an agent calls `context_query` with question "What tests cover validate_token?"
THEN the system SHALL look up `validate_token` in the test coverage index
AND return the list of test functions with file:line references

#### Scenario: Unrecognized Question

GIVEN an agent calls `context_query` with a question that cannot be parsed into any known query type
WHEN the system attempts to route the question
THEN the system SHALL return a message explaining the supported query types
AND list examples of valid questions

### Requirement: `read_context_section` Tool

The system SHALL provide a `read_context_section` tool that reads a specific section of a file from the pre-cached content snapshot.

#### Scenario: Read Default Range

GIVEN a `ContextMap` with a cached snapshot of `src/auth.py`
WHEN an agent calls `read_context_section` with `{ "path": "src/auth.py" }`
THEN the system SHALL return the first 50 lines of the file

#### Scenario: Read Custom Range

GIVEN a `ContextMap` with a cached snapshot of `src/auth.py`
WHEN an agent calls `read_context_section` with `{ "path": "src/auth.py", "start_line": 40, "max_lines": 20 }`
THEN the system SHALL return lines 40-59 of the file

#### Scenario: Read Beyond Max Lines

GIVEN a file longer than 200 lines
WHEN an agent calls `read_context_section` with `max_lines` > 200
THEN the system SHALL cap the returned lines at 200
AND append a footer: `... (showing N of M lines)`

#### Scenario: File Not in Snapshot

GIVEN a path that is not in the pre-cached file snapshots
WHEN an agent calls `read_context_section` with that path
THEN the system SHALL return an error message indicating the file was not cached
AND suggest the path may be outside the repository or generated

#### Scenario: Invalid Path

GIVEN a path with directory traversal components (`../`)
WHEN an agent calls `read_context_section` with that path
THEN the system SHALL reject the request
AND return a path-safety error

### Requirement: `find_references` Tool

The system SHALL provide a `find_references` tool that finds all references to a symbol across the codebase.

#### Scenario: Find All References

GIVEN a `ContextMap` with reference entries for `validate_token` in 3 files
WHEN an agent calls `find_references` with `{ "symbol": "validate_token" }`
THEN the system SHALL return the 3 references with file:line:context for each

#### Scenario: Find References in Specific File

GIVEN a `ContextMap` with reference entries for `validate_token` across 3 files
WHEN an agent calls `find_references` with `{ "symbol": "validate_token", "file": "src/api/handlers.py" }`
THEN the system SHALL return only references in `src/api/handlers.py`

#### Scenario: Symbol Not Found

GIVEN a symbol with no references in the CRG
WHEN an agent calls `find_references` with that symbol name
THEN the system SHALL return a message indicating no references were found
AND suggest checking the symbol name (case-sensitive)

### Requirement: `find_definition` Tool

The system SHALL provide a `find_definition` tool that locates where a symbol is defined in the codebase.

#### Scenario: Find Function Definition

GIVEN a `ContextMap` with a definition for `validate_token`
WHEN an agent calls `find_definition` with `{ "symbol": "validate_token" }`
THEN the system SHALL return the file, line number, signature, and docstring of the definition

#### Scenario: Find Class Definition

GIVEN a `ContextMap` with a definition for `AuthService`
WHEN an agent calls `find_definition` with `{ "symbol": "AuthService" }`
THEN the system SHALL return the file, line number, class signature, and list of methods

#### Scenario: Ambiguous Symbol Name

GIVEN multiple definitions with the same name across different files
WHEN an agent calls `find_definition` with that symbol name
THEN the system SHALL return all definitions with their file paths
AND let the caller disambiguate

#### Scenario: Symbol Not Defined

GIVEN a symbol with no definition in the CRG (external library or undefined)
WHEN an agent calls `find_definition` with that symbol name
THEN the system SHALL return a message indicating no definition was found

### Requirement: `show_diff_context` Tool

The system SHALL provide a `show_diff_context` tool that returns a structured summary of the PR diff and affected symbols.

#### Scenario: Show Diff Context

GIVEN a `ContextMap` with a populated `diff_context` (2 changed files, 3 affected symbols)
WHEN an agent calls `show_diff_context` with no parameters
THEN the system SHALL return a structured summary listing:
- Changed files with additions/deletions counts
- Hunks with line ranges
- Affected symbols and their callers

#### Scenario: No Diff Available

GIVEN a `ContextMap` built without a diff (diff_context is None)
WHEN an agent calls `show_diff_context` with no parameters
THEN the system SHALL return a message indicating no diff context is available
AND suggest this is a full-repository scan, not a PR review

### Requirement: Tool Registration in Agent Builder

The system SHALL register all 5 query tools on the agent when `context_map` is provided to `build_agent()`.

#### Scenario: Register All Tools

GIVEN a `ContextMap` instance
WHEN `build_agent()` is called with `context_map: Some(&map)`
THEN the system SHALL register all 5 query tools on the agent
AND the tool descriptions SHALL be included in the system prompt

#### Scenario: No Tools Without Context Map

GIVEN no `ContextMap` instance
WHEN `build_agent()` is called with `context_map: None`
THEN the system SHALL NOT register any CRG query tools
AND the agent SHALL operate in legacy mode with traditional tools only

#### Scenario: Tool Descriptions in Preamble

GIVEN a `ContextMap` registered on the agent
WHEN the agent system prompt is assembled
THEN the system SHALL include the tool usage preamble describing all 5 query tools
AND specify that tools read from pre-computed data (not live filesystem)
