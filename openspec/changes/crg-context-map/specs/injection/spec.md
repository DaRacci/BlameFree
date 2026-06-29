# Delta for Prompt Injection Strategy

## ADDED Requirements

### Requirement: `{context_map}` Template Variable

The system SHALL support a `{context_map}` template variable in prompt templates that resolves to the compact text rendering of the CRG.

#### Scenario: Inject Context Map into System Prompt

GIVEN a prompt template containing `{context_map}` and a `ContextMap` with `use_context_injection: true`
WHEN the agent system prompt is assembled
THEN the system SHALL render the compact text via `context_map.render_compact(2000)`
AND substitute `{context_map}` with the rendered text
AND the total prompt SHALL include the context map as an injected section

#### Scenario: Empty Template Variable

GIVEN a prompt template containing `{context_map}` but `use_context_injection: false` or no `ContextMap` available
WHEN the agent system prompt is assembled
THEN the system SHALL replace `{context_map}` with an empty string
OR a placeholder message: `"No context map available — use query tools for code context."`

#### Scenario: All 4 Agent Roles Receive Context

GIVEN 4 agent roles (SA, CL, AR, SEC) each with their own prompt template
WHEN context map injection is enabled
THEN the system SHALL inject the same `{context_map}` into all 4 agent system prompts
AND each agent SHALL have access to the full context map

### Requirement: PageRank-Based Context Ranking

The system SHALL rank files and symbols by PageRank on the file dependency graph to prioritize the most relevant context within the token budget.

#### Scenario: PageRank Computation

GIVEN a dependency graph with 100 files and import edges between them
WHEN the system builds the context map
THEN the system SHALL compute PageRank scores for each file node
AND store the scores in the `ranking` section of the `ContextMap`

#### Scenario: Ranked Symbol Selection

GIVEN a token budget of 2000 tokens and 5000 tokens of compact text
WHEN `context_map.render_compact(2000)` is called
THEN the system SHALL sort symbols by their file's PageRank score
AND include only the highest-ranked symbols until the 2000-token budget is filled
AND if the budget is exhausted mid-definition, include the full last definition

#### Scenario: Diff-Aware PageRank Boost

GIVEN a diff that modifies `src/auth.py`
WHEN computing PageRank for ranked selection
THEN the system SHALL add a boost multiplier to files changed by the diff
AND also boost files that directly import from or are imported by changed files
AND include all changed symbols regardless of PageRank rank (they are mandatory)

#### Scenario: Test File Inclusion

GIVEN test files that cover changed symbols (found in `test_coverage`)
WHEN computing ranked selection
THEN the system SHALL include test file references alongside their covered symbols
AND include the test file in the file structure section

### Requirement: Budget-Aware Context Selection

The system SHALL select context within a configurable token budget, with dynamic expansion for complex changes.

#### Scenario: Default Budget (2K Tokens)

GIVEN no explicit token budget is provided
WHEN `context_map.render_compact()` is called
THEN the system SHALL use a default budget of 2000 tokens

#### Scenario: Custom Budget

GIVEN a token budget of 4000 tokens is explicitly specified
WHEN `context_map.render_compact(4000)` is called
THEN the system SHALL include approximately 4000 tokens of context
AND SHALL NOT exceed the specified budget by more than 5%

#### Scenario: Dynamic Expansion for Complex Changes

GIVEN a diff with >10 changed files spanning multiple directories
WHEN the system evaluates diff complexity
THEN the system SHALL permit the token budget to expand to a maximum of 4000 tokens
AND log the expanded budget for benchmarking

#### Scenario: Budget Exhaustion with Truncation

GIVEN a token budget of 500 tokens in a large repository
WHEN `context_map.render_compact(500)` is called
THEN the system SHALL include the `=== FILE STRUCTURE ===` section (abbreviated if needed)
AND include **all** diff-affected symbols
AND include only the highest-ranked non-diff symbols
AND include the `=== DIFF CHANGES ===` section
AND omit the `=== DEPENDENCIES ===` section if budget is exceeded
AND append a footer: `(Context map truncated — use query tools for deeper exploration)`

### Requirement: Hybrid Mode Toggle

The system SHALL support three modes: injection only, tools only, and hybrid (inject + tools).

#### Scenario: Injection Only Mode

GIVEN `use_context_injection: true` and no query tools registered
WHEN the agent is built
THEN the system SHALL inject the compact text into the prompt
AND the agent SHALL have no query tools available
AND the agent SHALL rely entirely on the injected context

#### Scenario: Tools Only Mode

GIVEN `use_context_injection: false` but query tools are registered
WHEN the agent is built
THEN the system SHALL NOT inject context into the prompt
AND the agent SHALL have all 5 query tools available
AND the agent SHALL make tool calls to retrieve code context

#### Scenario: Hybrid Mode

GIVEN `use_context_injection: true` and all 5 query tools registered
WHEN the agent is built
THEN the system SHALL inject the compact text into the prompt
AND register all 5 query tools as fallback
AND set `max_turns: 2` to prevent tool-loop issues

### Requirement: Configuration Toggle

The system SHALL provide a configuration option to enable/disable context map injection and select the mode.

#### Scenario: Config-Driven Mode Selection

GIVEN a configuration file `config.toml` with `context_map_mode = "hybrid"`
WHEN the system builds agents
THEN the system SHALL operate in hybrid mode (inject + query tools)

GIVEN `context_map_mode = "injection_only"`
WHEN the system builds agents
THEN the system SHALL operate in injection-only mode

GIVEN `context_map_mode = "disabled"`
WHEN the system builds agents
THEN the system SHALL NOT inject context or register query tools
AND SHALL fall back to legacy live tool behavior

#### Scenario: CLI Override

GIVEN a CLI flag `--context-map-mode=hybrid`
WHEN the system builds agents
THEN the CLI flag SHALL override the config file value
AND the system SHALL operate in the specified mode
