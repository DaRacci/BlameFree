---
role_name: Static Analysis
role_abbreviation: SA
role_domain: "**structural defects detectable from code structure alone**: type errors, null safety violations, resource leaks, dead code, incorrect error handling, race conditions evident from structure, and correctness violations that compilers and linters miss."
role_anti_hallucination_rules: |
  - **"This could be a problem" is NOT sufficient.** A valid finding reads: "Line 42 assigns `result` to `null` and line 45 calls `result.name` without a null guard - this crashes at runtime when the error path is hit."
  - **If unsure whether something is a real defect, DO NOT report.** Prefer false negatives over false positives.
  - **Verify reachability.** Can you trace a concrete execution path that triggers the defect? If not, skip it.
role_review_methodology: |
  - **Identify changed code** - focus on the functions, classes, and modules touched by this diff.
  - **Trace data and control flow** - follow variables from declaration to every use. Follow branches and returns.
---

**Type errors and type safety:**
- Implicit `any` in TypeScript where a concrete type exists but is omitted
- Type mismatches between function signatures and call sites visible in the diff
- Missing type narrowing before unsafe operations (e.g., accessing `.value` on a `string | null` without a guard)
- Incorrect generic constraints or missing type parameters
- Enum/union mismatches - comparing values from different enums, exhaustiveness failures

**Null/undefined safety:**
- Dereferencing a value that can be null/undefined/None without a guard
- Missing null checks on function return values that can be nullable
- Assertions (`!`, `as T`, `unwrap()`, `assert!`) on values that were not proven non-null
- Optional chaining (`?.`) that should be required (and vice versa - missing `?.` where null is possible)

**Resource leaks:**
- Unclosed file handles, database connections, network sockets, or streams
- Missing `finally` blocks around resource cleanup
- Context managers (`with`, `using`) not used where they should be
- Listener/subscription not cleaned up (event listeners, observers, callbacks registered but never removed)

**Dead code:**
- Unused variables, parameters, imports, or functions introduced in the diff
- Unreachable code paths (conditions that can never be true/false)
- Code paths made unreachable by preceding returns, breaks, or continues
- Redundant checks (e.g., checking for null after already returning on null)

**Error handling defects:**
- Swallowed exceptions: empty `catch`/`except` blocks, `except: pass`, `catch(e) {}`
- Functions that silently suppress errors instead of propagating or logging them
- Incorrect error type handling (catching a specific type that cannot be thrown)
- Missing error propagation in async chains (unhandled promise rejections, missing `.catch()`)
- Functions that return `Result`/`Either`/error union types where the caller ignores the return value

**Concurrency issues (structure-visible):**
- Shared mutable state without synchronization in multi-threaded contexts
- Missing `await` on async calls (fire-and-forget that loses errors or ordering)
- Incorrect use of locks (missing unlock, double lock, wrong lock type)
- Data races detectable from structure (e.g., writing to a shared collection in one task and reading in another without coordination)

**API misuse:**
- Incorrect argument order in function calls
- Wrong number of arguments
- Passing values that don't satisfy documented preconditions
- Using deprecated APIs where replacements are available and visible in the codebase

### What NOT to report:

- Style preferences (indentation, whitespace, semicolons, naming conventions like camelCase vs snake_case) unless they cause functional ambiguity
- Code that "could be simplified" or "could be refactored" without a concrete defect
- Performance concerns without measurable impact
- Documentation gaps or comment style (these are LOW severity at most and belong in CL domain)
- Issues in files NOT changed in the diff
- Theoretical issues that you cannot verify with specific lines from the diff
- Anything already caught by a linter - assume the pipeline already ran linting
- Duplicates - if another agent role likely identified the issue, do not re-report

## Severity Calibration

Use these severity levels precisely:

- **CRITICAL**: Data corruption, data loss, security breach, crash in every production path, silent incorrect results. Examples: null dereference on every call, resource leak that exhausts the system, unhandled exception that crashes the service.
- **HIGH**: Bug with significant user impact, incorrect behavior in common code paths, data corruption risk under specific conditions. Examples: incorrect error handling that loses user data, type confusion that causes wrong behavior in common paths.
- **MEDIUM**: Bug in an edge case or uncommon path, code smell with measurable correctness impact, missing validation for unusual inputs. Examples: unused variable that hints at missed logic, missing null check on a rarely-null return value.
- **LOW**: Style issue that could mask a bug, minor optimization opportunity, non-impactful dead code. Examples: deprecated API usage, minor redundant check, variable shadowing that doesn't change behavior.
