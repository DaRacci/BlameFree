# Delta for Agent Tools

## ADDED Requirements

### Requirement: ShellTool
The system SHALL provide a ShellTool for executing shell commands during code review.

#### Scenario: Run grep command
- GIVEN an agent needs to search for patterns in the codebase
- WHEN the agent invokes ShellTool with a grep command
- THEN it spawns sh -c with the command
- AND it returns stdout of the command

#### Scenario: Output cap enforcement
- GIVEN a command produces output exceeding 100KB
- WHEN ShellTool captures the output
- THEN it returns an OutputTooLarge error

#### Scenario: Timeout enforcement
- GIVEN a command runs longer than 30 seconds
- WHEN the tool enforces the timeout
- THEN it returns a TimeoutElapsed error

### Requirement: ReadFileTool
The system SHALL provide a ReadFileTool for reading repo files during code review.

#### Scenario: Read file with line range
- GIVEN an agent needs to read a specific section of a file
- WHEN the agent invokes ReadFileTool with path, start_line, and max_lines
- THEN it reads the file from the repository root
- AND it returns the requested lines

#### Scenario: Path traversal prevention
- GIVEN an agent attempts to read a file outside the repo root
- WHEN ReadFileTool validates the path
- THEN it rejects the request with PathOutsideRepo error

### Requirement: GitTool
The system SHALL provide a GitTool for repository operations during code review.

#### Scenario: View commit history
- GIVEN an agent needs to understand recent changes
- WHEN the agent invokes GitTool with operation=log
- THEN it runs git log --oneline -n 20
- AND it returns the formatted log

#### Scenario: Generate diff
- GIVEN an agent needs to see changes between refs
- WHEN the agent invokes GitTool with operation=diff, base, and head
- THEN it runs git diff base...head --no-color
- AND it returns the unified diff output

### Requirement: MCPTool
The system SHALL provide an MCPTool for invoking external MCP servers during code review.

#### Scenario: Call MCP tool
- GIVEN a configured MCP server
- WHEN the agent invokes MCPTool with a tool_name and arguments
- THEN it POSTs to the server's /call_tool endpoint
- AND it returns the JSON-RPC 2.0 response

#### Scenario: Not configured
- GIVEN no MCP server is configured
- WHEN the agent invokes MCPTool
- THEN it returns a NotConfigured error

### Requirement: Per-Role Tool Assignment
The system SHALL assign different tool sets to each agent role.

#### Scenario: Static Analysis role
- GIVEN a Static Analysis agent role
- WHEN the system creates the agent
- THEN it assigns [ShellTool, ReadFileTool] to the SA role

#### Scenario: Code Logic role
- GIVEN a Code Logic agent role
- WHEN the system creates the agent
- THEN it assigns [ShellTool, ReadFileTool, GitTool] to the CL role

### Requirement: Tool Call Budget
The system SHALL enforce a tool call budget per review to prevent runaway tool use.

#### Scenario: Budget limits
- GIVEN a default budget of 50 total calls, 20 per-tool
- WHEN an agent exceeds the per-tool limit
- THEN it warns the agent but allows the call (soft-stop)
