# GitTool Specification

## Type: Implementation Spec

## 1. GitCleanTool

### Args
```rust
#[derive(Deserialize, JsonSchema)]
struct GitCleanArgs {
    /// Absolute or relative path to the repository.
    repo_path: String,
}
```

### Behavior
Runs `git -C {repo_path} clean -fdx` which removes:
- Untracked files
- Untracked directories
- Files matching `.gitignore` patterns (since `-x` overrides ignore)

### Output
`String` — stdout of the `git clean` command (typically blank or short summary).

### Error
`GitError` — see §4.

## 2. GitDiffTool

### Args
```rust
#[derive(Deserialize, JsonSchema)]
struct GitDiffArgs {
    /// Absolute or relative path to the repository.
    repo_path: String,
    /// Base git ref (e.g. "main", "origin/main", "HEAD~1").
    base: String,
    /// Head git ref (e.g. "feature-branch", "HEAD").
    head: String,
}
```

### Behavior
Runs `git -C {repo_path} diff {base}...{head} --no-color` using the
three-dot symmetric difference notation. The `--no-color` flag ensures
the output is always plain text.

### Output
`String` — unified diff output. Empty string if no differences.

### Error
`GitError` — see §4.

## 3. Extra Git Tools (Future)

These are not implemented in the initial change but follow the same pattern:

| Tool | Command | Args |
|---|---|---|
| `GitLogTool` | `git -C {path} log --oneline {base}..{head}` | `repo_path`, `base`, `head` |
| `GitStatusTool` | `git -C {path} status --porcelain` | `repo_path` |
| `GitCheckoutTool` | `git -C {path} checkout {ref}` | `repo_path`, `ref` |

## 4. GitError

```rust
#[derive(Debug)]
enum GitError {
    /// IO error spawning or communicating with git process.
    CommandFailed(std::io::Error),
    /// Git exited with a non-zero exit code.
    NonZeroExit(i32, String),
    /// The operation exceeded the timeout.
    TimeoutElapsed,
}
```

Implement `std::error::Error` and `std::fmt::Display`.

## 5. Execution Strategy

Git operations use `std::process::Command` wrapped in
`tokio::task::spawn_blocking` to avoid blocking the async runtime. Timeouts
are enforced via `tokio::time::timeout`.

```rust
tokio::time::timeout(Duration::from_secs(60), async {
    tokio::task::spawn_blocking(move || {
        std::process::Command::new("git")
            .args(["-C", &args.repo_path, "clean", "-fdx"])
            .output()
    })
    .await
    .map_err(|join_err| GitError::CommandFailed(/* ... */))?
})
```
