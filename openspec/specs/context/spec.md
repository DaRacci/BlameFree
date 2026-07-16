# context Specification

## Purpose
Repository context gathering for code review, including GitHub URL parsing, PR diff retrieval, and branch checkout.
## Requirements
### Requirement: Repo URL Parsing
The system SHALL parse GitHub repository URLs to extract owner, repo name, and PR number.

#### Scenario: Valid GitHub PR URL
- GIVEN a URL `https://github.com/owner/repo/pull/123`
- WHEN the system parses the URL
- THEN it extracts `owner = "owner"`, `repo = "repo"`, `pr_number = 123`

#### Scenario: Invalid URL format
- GIVEN a URL that is not a valid GitHub PR URL
- WHEN the system attempts to parse it
- THEN it returns an error indicating the URL format is invalid
- AND the review job transitions to `failed` status

#### Implementation
```rust
fn parse_github_url(url: &str) -> Result<(String, String, u32)> {
    let re = Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/pull/(\d+)$")?;
    let caps = re.captures(url).ok_or_else(|| anyhow!("Invalid GitHub PR URL"))?;
    let owner = caps[1].to_string();
    let repo = caps[2].to_string();
    let pr_number: u32 = caps[3].parse()?;
    Ok((owner, repo, pr_number))
}
```

### Requirement: Repository Cloning
The system SHALL clone repositories for context gathering.

#### Scenario: First-time clone
- GIVEN a repo URL that has not been cached before
- WHEN the system clones the repository
- THEN it performs a shallow clone (`git clone --depth 1`) to the cache directory
- AND it fetches the PR branch (`git fetch origin pull/{pr_number}/head:{branch}`)
- AND it checks out the PR branch

#### Scenario: Cached repo
- GIVEN a repo that has been cloned before in the cache
- WHEN the system needs to gather context
- THEN it runs `git fetch origin` to update the existing clone
- AND it fetches and checks out the PR branch
- AND it avoids re-cloning from scratch

#### Implementation
```rust
pub async fn clone_or_fetch_repo(cache_path: &Path, owner: &str, repo: &str, pr_number: u32) -> Result<()> {
    let repo_url = format!("https://github.com/{}/{}.git", owner, repo);

    if cache_path.join(".git").exists() {
        // Update existing clone
        run_git(cache_path, &["fetch", "origin"]).await?;
    } else {
        // Shallow clone
        run_git_in(cache_path.parent(), &[
            "clone", "--depth", "1", &repo_url, cache_path,
        ]).await?;
    }

    // Fetch PR branch
    let refspec = format!("pull/{pr_number}/head:pr-{pr_number}");
    run_git(cache_path, &["fetch", "origin", &refspec]).await?;

    // Checkout PR branch
    run_git(cache_path, &["checkout", &format!("pr-{pr_number}")]).await?;

    Ok(())
}
```

#### Timeout behavior
- Clone/fetch operations SHALL timeout after 60 seconds
- If the timeout is exceeded, the review job SHALL transition to `failed` status with an appropriate error message

### Requirement: Diff Generation
The system SHALL generate a PR diff from the cloned repository.

#### Scenario: Server-generated diff
- GIVEN a cloned repo with PR branch checked out
- WHEN no inline diff was provided in the request
- THEN the system runs `git diff {base_branch}...HEAD` to compute the diff
- AND it stores the diff string in `RepoContext`

#### Scenario: Inline diff override
- GIVEN a request includes an explicit `diff` field
- WHEN the system gathers context
- THEN it uses the provided diff directly instead of generating it from git

#### Implementation
```rust
pub async fn get_diff(cache_path: &Path, base_branch: &str) -> Result<String> {
    let output = run_git_output(cache_path, &["diff", &format!("{}...HEAD", base_branch)]).await?;
    Ok(String::from_utf8(output.stdout)?)
}
```

### Requirement: Changed File Detection
The system SHALL extract the list of changed files from the diff.

#### Scenario: Parse changed files
- GIVEN a git diff output string
- WHEN the system detects changed files
- THEN it parses the diff for `+++ b/...` or `--- a/...` lines to extract file paths
- AND it deduplicates the file list
- AND it stores the list in `RepoContext.changed_files`

### Requirement: Tech-Stack Detection
The system SHALL detect the primary language and technology stack of the repository.

#### Scenario: Rust project
- GIVEN a repository containing a `Cargo.toml` file
- WHEN the system detects the tech stack
- THEN it returns language = `"Rust"`
- AND it parses `[dependencies]` section for key framework/libraries (e.g., tokio, axum, actix-web)

#### Scenario: Node.js project
- GIVEN a repository containing a `package.json` file
- WHEN the system detects the tech stack
- THEN it returns language = `"JavaScript"` or `"TypeScript"` (if `tsconfig.json` present)
- AND it parses `dependencies` and `devDependencies` for key frameworks

#### Detection priority order
The system SHALL check files in the following order and return the first match:

1. `Cargo.toml` -> Rust
2. `go.mod` -> Go
3. `Cargo.toml` + `package.json` -> Rust/JavaScript (multi-language)
4. `pyproject.toml` -> Python
5. `requirements.txt` -> Python
6. `package.json` -> JavaScript/TypeScript
7. `build.gradle` or `build.gradle.kts` -> Kotlin/Java
8. `pom.xml` -> Java
9. `Gemfile` -> Ruby
10. `*.csproj` -> C#
11. `CMakeLists.txt` -> C/C++
12. Default: `"Unknown"`

### Requirement: Module Analysis
The system SHALL identify key source modules/directories in the repository.

#### Scenario: Standard Rust project structure
- GIVEN a Rust project with `src/main.rs`, `src/lib.rs`, `src/routes/`, `src/models/`
- WHEN the system analyzes modules
- THEN it returns `["src/routes", "src/models"]` (top-level source directories plus entry points)

#### Scenario: No recognized source directory
- GIVEN a repository without a standard `src/`, `lib/`, or `app/` directory
- WHEN the system analyzes modules
- THEN it returns an empty `Vec<String>` and logs an info message

#### Detection logic
```
1. Check for well-known source roots: src/, lib/, app/, cmd/, internal/
2. List immediate subdirectories of the first existing source root
3. Also identify entry point files: main.rs, main.go, index.js, app.py, server.py, main.py
4. Limit to top 20 modules to avoid overly large lists
```

### Requirement: CRG Integration (Optional)
The system SHALL optionally invoke `code-review-graph detect-changes` for call-graph context.

#### Scenario: CRG available
- GIVEN `code-review-graph` is installed on the server's PATH
- WHEN the system gathers context
- THEN it runs `code-review-graph detect-changes --repo {cache_path}`
- AND it captures stdout as the call graph context string
- AND it injects the result into the `{call_graph}` template variable

#### Scenario: CRG not available
- GIVEN `code-review-graph` is NOT installed on the server's PATH
- WHEN the system attempts CRG integration
- THEN it logs an info message: "code-review-graph not found on PATH, skipping"
- AND it continues with the remaining context gathering steps
- AND the `{call_graph}` template variable is left empty

### Requirement: Template Variable Injection
The system SHALL convert gathered context into template variables for prompt injection.

#### Scenario: Render prompt with context
- GIVEN a `RepoContext` with repo = "owner/repo", language = "Rust", tech_stack = ["Tokio", "Axum"]
- WHEN the system creates template variables
- THEN it produces `HashMap` entries:
  - `"repo"` -> `"owner/repo"`
  - `"language"` -> `"Rust"`
  - `"tech_stack"` -> `"Tokio, Axum"`
  - `"modules"` -> `"src/routes, src/models"`
  - `"changed_files"` -> `"src/main.rs, src/routes/review.rs"`
  - `"call_graph"` -> `""` (empty if CRG unavailable)

#### Scenario: Apply to agent prompt
- GIVEN a prompt template containing `{repo}` and `{language}`
- WHEN the system renders the prompt with context variables
- THEN all `{repo}` and `{language}` placeholders are replaced with their values
- AND the rendered prompt is used as the agent's system prompt

### Requirement: Caching Behavior
The system SHALL cache cloned repositories to avoid repeated network operations.

#### Scenario: Cache hit
- GIVEN a repo that has been cloned in the last hour
- WHEN a new review request arrives for the same repo
- THEN the system uses the cached clone (with `git fetch` to update)
- AND the clone step completes in < 2 seconds instead of > 10 seconds

#### Scenario: Cache directory management
- GIVEN the `--repos-cache` directory exists
- WHEN the server starts
- THEN it creates the cache directory if it doesn't exist (with `create_dir_all`)
- AND it cleans up old repo clones older than 24 hours (optional, via periodic task)
- AND it enforces a maximum cache size (configurable, default 1GB)

