# Spec: Agent Tool Implementations

## ShellTool

### Input

```json
{
  "command": "string  // shell command to run (e.g. \"grep -r unsafe src/\")"
}
```

### Output

- `Ok(String)`: stdout of the command.
- `Err(ShellError)`:

| Variant | Condition |
|---------|-----------|
| `SpawnFailed(String)` | Subprocess could not be spawned |
| `NonZeroExit(i32, String)` | Command exited with non-zero code (includes stderr) |
| `TimeoutElapsed` | Command exceeded timeout (default 30s) |
| `OutputTooLarge(usize)` | stdout exceeded 100KB cap |

### Safety

- Runs via `sh -c` — shell injection is inherent; use with caution. Future: deny-list.
- Working directory is confined to the repo root.
- 30s hard timeout prevents runaway processes.

### Limitations

- No interactive commands (no stdin).
- 100KB output cap prevents context-window overflow.
- No environment variable isolation (inherits harness environment).

---

## ReadFileTool

### Input

```json
{
  "path": "string",           // path relative to repo root
  "start_line": "uint | null", // 1-indexed start line (default: 1)
  "max_lines": "uint | null"   // max lines to read (default: 200)
}
```

### Output

- `Ok(String)`: File content, possibly truncated with footer: `... (showing N of M lines)`.
- `Err(ReadFileError)`:

| Variant | Condition |
|---------|-----------|
| `IoError(String)` | File not found, permissions, etc. |
| `PathOutsideRepo(String)` | Canonicalized path is not under `repo_root` |
| `FileTooLarge(u64)` | File exceeds 1MB size limit |

### Safety

- **Path traversal protection**: Both `repo_root` and the target path are canonicalized via `dunce::canonicalize`. Any path whose canonical form does not start with `repo_root` is rejected.
- **Size limit**: Files > 1MB are rejected before reading to prevent OOM.

### Limitations

- Binary files are read as text (lossy UTF-8 conversion in `read_to_string`).
- Line-range reading is implemented after reading the full file (no streaming).

---

## GitTool

### Input

```json
{
  "operation": {
    "type": "log | diff | show | status",
    "fields": {
      "base": "string (diff only)",  // base git ref
      "head": "string (diff only)",  // head git ref
      "ref": "string (show only)"    // ref to show
    }
  }
}
```

### Supported Operations

| Operation | Git Command | Description |
|-----------|-------------|-------------|
| `log` | `git log --oneline -n 20` | Recent commit history |
| `diff` | `git diff base...head --no-color` | Changes between two refs |
| `show` | `git show <ref> --no-color` | Show a specific commit |
| `status` | `git status --short` | Working tree status |

### Output

- `Ok(String)`: Raw git command stdout.
- `Err(GitToolError)`:

| Variant | Condition |
|---------|-----------|
| `CommandFailed(String)` | Git binary not found or IO error |
| `NonZeroExit(i32, String)` | Git command failed (includes stderr) |
| `TimeoutElapsed` | Operation exceeded timeout (default 30s) |

### Safety

- Path safety via `-C <repo_root>` flag, ensuring git operations stay within the repo.
- No `--exec-path` or other git config injection possible.

---

## MCPTool

### Input

```json
{
  "tool_name": "string",   // name of the MCP tool to invoke
  "arguments": "string"    // JSON-encoded arguments
}
```

### Output

```json
{
  "success": "bool",
  "result": "string",
  "error": "string | null"
}
```

### Protocol

- HTTP POST to `{server_url}/call_tool` with JSON-RPC 2.0 payload.
- HTTP POST to `{server_url}/list_tools` for tool discovery.
- Tool definitions are cached in `tool_definitions` after `fetch_tools()`.

### Configuration

```toml
[mcp_tools.search]
server_url = "http://localhost:3000/mcp"
api_key = "sk-..."       # optional
timeout_secs = 30
optional = true
```

### Error Handling

| Error | When Raised |
|-------|-------------|
| `NotConfigured(String)` | No MCP server configured |
| `RequestFailed(String)` | HTTP request failed or invalid response |
| `TimeoutElapsed` | Request exceeded configured timeout |
| `ServerError(String)` | MCP server returned JSON-RPC error |

### Caching

Tool definitions fetched from `list_tools` are held in memory on `MCPTool`. They persist for the lifetime of the tool instance.
