# Project Rename: crb → riv / review-harness → BlameFree

> **For Hermes:** Use subagent-driven-development to execute this plan task-by-task.

**Goal:** Rename the project from "review-harness" to "BlameFree", rename all crates from `crb-*` to `riv-*`, rename the main CLI binary to `riv`, and update all internal Rust references.

**Architecture:** Parallel directory renames → sequential Cargo.toml fixes → parallel Rust source rewrites (per-crate subagents) → documentation sweep → snapshot regeneration → full `cargo check --workspace` verification.

**Tech Stack:** Rust workspace, 15 crates, insta snapshots, trybuild, proc-macros with hardcoded crate paths.

**Crate Naming Map:**

| Old Directory        | New Directory        | Old Package Name     | New Package Name     | Binary Name(s)      |
|----------------------|----------------------|----------------------|----------------------|---------------------|
| `crates/crb-agents`  | `crates/riv-agents`  | `crb-agents`         | `riv-agents`         | —                   |
| `crates/crb-auditor` | `crates/riv-auditor` | `crb-auditor`        | `riv-auditor`        | —                   |
| `crates/crb-benchmark` | `crates/riv-benchmark` | `crb-benchmark`    | `riv-benchmark`      | `crb-benchmark` → `riv-benchmark` |
| `crates/crb-cache`   | `crates/riv-cache`   | `crb-cache`          | `riv-cache`          | —                   |
| `crates/crb-consensus` | `crates/riv-consensus` | `crb-consensus`    | `riv-consensus`      | —                   |
| `crates/crb-harness` | `crates/riv-harness` | `crb-harness`        | `riv-harness`        | `crb-harness` → `riv` |
| `crates/crb-macros`  | `crates/riv-macros`  | `crb-macros`         | `riv-macros`         | —                   |
| `crates/crb-reporting` | `crates/riv-reporting` | `crb-reporting`    | `riv-reporting`      | —                   |
| `crates/crb-rules`   | `crates/riv-rules`   | `crb-rules`          | `riv-rules`          | —                   |
| `crates/crb-shared`  | `crates/riv-shared`  | `crb-shared`         | `riv-shared`         | —                   |
| `crates/crb-tools`   | `crates/riv-tools`   | `crb-tools`          | `riv-tools`          | —                   |
| `crates/crb-types`   | `crates/riv-types`   | `crb-types`          | `riv-types`          | —                   |
| `crates/crb-webui-backend` | `crates/riv-webui-backend` | `crb-webui-backend` | `riv-webui-backend` | `crb-webui` → `riv-webui` |
| `crates/crb-webui-frontend` | `crates/riv-webui-frontend` | `crb-webui-frontend` | `riv-webui-frontend` | — |
| `crates/crb-webui-shared` | `crates/riv-webui-shared` | `crb-webui-shared` | `riv-webui-shared` | — |
| `crates/riv-stor`    | (unchanged)          | `riv-stor`           | (unchanged)          | —                   |

---

## Architecture Constraints

The critical constraint is **ordering of crate dependency resolution**. Since all crates depend on `riv-types` and many depend on `riv-shared`, these must be renamed first. The proc-macro crate `riv-macros` generates code with hardcoded `::crate_name::` paths — it's the trickiest.

**Dependency graph (simplified):**
- `riv-types` — leaf (depends only on std lib + `riv-macros` optional)
- `riv-macros` — leaf (depends on `riv-types`, `riv-cache`)
- `riv-cache` — depends on `riv-types`, `riv-macros`
- `riv-shared` — depends on `riv-cache`, `riv-types`
- `riv-agents` — depends on `riv-reporting`, `riv-types`
- `riv-reporting` — depends on `riv-types`
- `riv-rules` — leaf (depends on nothing internal)
- `riv-tools` — depends on `riv-shared`, `riv-types`
- `riv-consensus` — depends on `riv-agents`, `riv-cache`, `riv-reporting`, `riv-shared`, `riv-types`
- `riv-auditor` — depends on `riv-shared`, `riv-types`
- `riv-harness` — depends on most crates
- `riv-webui-shared` — depends on `riv-macros`, `riv-types`
- `riv-webui-backend` — depends on most crates
- `riv-webui-frontend` — depends on `riv-types`, `riv-webui-shared`, `riv-shared`

The pattern is systematic: **every** `crb_` in Rust source becomes `riv_`, **every** `crb-` in TOML package/dep names becomes `riv-`.

---

## Implementation Plan

### Phase 0: Cargo.lock removal (start fresh)

```bash
rm Cargo.lock
```

Avoids stale hash references to old `crb-` packages.

---

### Phase 1: Rename directories

**Task 1.1: Move all crate directories**

Each: `mv crates/crb-<name> crates/riv-<name>`

```bash
cd /data/workspace/projects/review-harness

# All 15 directory renames
for old in crb-agents crb-auditor crb-benchmark crb-cache crb-consensus crb-harness crb-macros crb-reporting crb-rules crb-shared crb-tools crb-types crb-webui-backend crb-webui-frontend crb-webui-shared; do
  new="${old/crb-/riv-}"
  mv "crates/$old" "crates/$new"
done
```

**Verification:** `ls crates/` shows all `riv-*` + `riv-stor`

---

### Phase 2: Update Cargo.toml files (15 crate files + root workspace)

Each crate's `Cargo.toml` needs:
1. `name = "crb-..."` → `name = "riv-..."`
2. Binary names: `name = "crb-harness"` → `name = "riv"`, `name = "crb-benchmark"` → `name = "riv-benchmark"`, `name = "crb-webui"` → `name = "riv-webui"`
3. All internal dependency references: `crb-... = { path = "../crb-..." }` → `riv-... = { path = "../riv-..." }`
4. All feature flags referencing crb- crates
5. The binary target feature in crb-benchmark: `binary = ["dep:clap", "dep:crb-harness", ...]` → `"dep:riv-harness"` etc.
6. Feature activation in webui-backend: `exp14_template_vars = ["crb-harness/exp14_template_vars"]` → `exp14_template_vars = ["riv-harness/exp14_template_vars"]`
7. `riv-stor/Cargo.toml` — still references `crb-types` → update to `riv-types`

**Task 2.1: Update each Cargo.toml**

The pattern is:

For package name:
```toml
# OLD
name = "crb-types"
# NEW
name = "riv-types"
```

For dependencies:
```toml
# OLD
crb-types = { path = "../crb-types" }
# NEW
riv-types = { path = "../riv-types" }
```

For feature flags:
```toml
# OLD
binary = ["dep:clap", "dep:crb-harness", "dep:crb-rules", "dep:crb-tools", ...]
# NEW
binary = ["dep:clap", "dep:riv-harness", "dep:riv-rules", "dep:riv-tools", ...]
```

For feature activation:
```toml
# OLD
exp14_template_vars = ["crb-harness/exp14_template_vars"]
# NEW
exp14_template_vars = ["riv-harness/exp14_template_vars"]
```

For seaorm-storage feature in crb-types (now riv-types):
```toml
# OLD
seaorm-storage = ["sea-orm/macros", "crb-macros"]
# NEW
seaorm-storage = ["sea-orm/macros", "riv-macros"]
```

For binary names:
```toml
# OLD (riv-harness)
[[bin]]
name = "crb-harness"
# NEW
[[bin]]
name = "riv"

# OLD (riv-benchmark)
[[bin]]
name = "crb-benchmark"
# NEW
[[bin]]
name = "riv-benchmark"

# OLD (riv-webui-backend)
[[bin]]
name = "crb-webui"
# NEW
[[bin]]
name = "riv-webui"
```

Also need to update the bench binary name in the benchmark crate's `[[bin]]` section (line 9 currently has `name = "crb-benchmark"`).

---

### Phase 3: Update all Rust source files

This is the bulk of the work. Every `use crb_` path and every `::crb_` qualified path must become `riv_`.

**Task 3.1: Update riv-macros proc-macro sources** (MOST CRITICAL)

File: `riv-macros/src/flatten.rs` — contains hardcoded `::crb_types::`, `::crb_cache::` paths
File: `riv-macros/src/cache.rs` — contains hardcoded `::crb_types::`, `::crb_cache::` paths
File: `riv-macros/src/lib.rs` — doc comments referencing `crb_macros::`

These must be updated FIRST because other crates will use these macros.

Replace all occurrences of `::crb_` with `::riv_` and `crb_macros::` with `riv_macros::` in the macro source.

You can use a sed replacement:
```bash
for f in riv-macros/src/lib.rs riv-macros/src/flatten.rs riv-macros/src/cache.rs; do
  sed -i 's/::crb_/::riv_/g; s/\bcrb_macros::/riv_macros::/g' "crates/$f"
done
```

**Task 3.2: Update all Rust use paths across all crates**

Pattern: `use crb_` → `use riv_` everywhere.

This can be batched with sed for speed, but since `linters/deny: unwrap_used` etc., we need to be careful. The safest approach is a workspace-wide sed:

```bash
# Replace all use paths and qualified paths
find crates -name '*.rs' -exec sed -i 's/\buse crb_/use riv_/g; s/\bcrate::crb_/crate::riv_/g' {} \;

# Replace all ::crb_ paths (mainly in macro-generated code and doc comments)
find crates -name '*.rs' -exec sed -i 's/::crb_/::riv_/g' {} \;

# Replace remaining string references like doc comments
find crates -name '*.rs' -exec sed -i 's/\bcrb_/riv_/g' {} \;
```

**NOTE:** The third sed is aggressive — it replaces ALL instances of `crb_` with `riv_` even in string literals and doc comments. This is actually what we want since all `crb_types`, `crb_shared`, `crb_cache`, `crb_*` should become `riv_*`.

**But caution:** we must NOT replace `crb_` inside third-party crate names. There are no third-party crates starting with `crb_`, so this is safe.

**Task 3.3: Update doc comments referencing "crb" in string form**

Some doc comments say things like:
- `//! \`crb-rules\` implements...`
- `/// \`crb_types::cost\` so they can be shared...`
- `/// the loaded [\`PromptLibrary\`](crb_agents::prompts::PromptLibrary).`
- `pub use crb_types::cost::...`

The sed above should handle most of these. But some may be in markdown-style links.

**Task 3.4: Update `riv-webui-frontend` special case**

File: `riv-webui-frontend/src/lib.rs` has:
```rust
#[serde(default = "crb_shared::default_model")]
```
The string form is used in a serde attribute — this also needs to become `"riv_shared::default_model"`.

Similarly in `riv-webui-backend/src/api/runs.rs`:
```rust
#[serde(default = "crb_shared::default_model")]
```

---

### Phase 4: Update documentation files

**Task 4.1: Root README.md**

Update title from `# review-harness` to `# BlameFree`.
Update `--bin crb-harness` references to `--bin riv`.

**Task 4.2: AGENTS.md**

Update project layout table, all `crb-*` crate names, doc references like `crb_agents::prompts::PromptLibrary`.

**Task 4.3: Crate READMEs**

Update each README.md in crate directories:
- `crb-agents/README.md` → update title
- `crb-benchmark/README.md` → update title + usage examples (`cargo run -p crb-benchmark` → `cargo run -p riv-benchmark`)
- `crb-harness/README.md` → update title + references to other crates
- `crb-reporting/README.md` → update title
- `crb-rules/README.md` → update title + `.crb/rules/` already uses `.riv/rules/` so that's fine
- `crb-webui-frontend/README.md` → update all references (crb-webui → riv-webui)

**Task 4.4: openspec/ design documents**

Update `openspec/eval-system-design.md` — replace all `crb-eval`, `crb-harness`, `crb-*` references.
Update other `openspec/` files referencing `review-harness` and `crb-`.

---

### Phase 5: Update `linters.toml` comment

File: `riv-tools/linters.toml` line 1: comment references `review-harness` → update to `BlameFree`.

---

### Phase 6: Regenerate insta snapshots

After all source changes, the insta snapshot `.snap` files contain `source: crates/crb_...` annotations on line 2 that will be stale.

```bash
INSTA_UPDATE=new cargo test --workspace 2>&1 | tail -20
cargo insta review  # accept all
```

### Phase 7: Update trybuild snapshots (if needed)

The trybuild `.stderr` files in `riv-macros/tests/fixtures/` should be checked. Their content doesn't reference `crb` directly, but the error message might include the old rustc path. Run:

```bash
TRYBUILD=overwrite cargo test -p riv-macros 2>&1 | tail -20
```

### Phase 8: Verify

```bash
cargo check --workspace
cargo nextest run
```

---

## Implementation Order Summary

| Step | Action | Tooling |
|------|--------|---------|
| 0 | Delete `Cargo.lock` | `rm` |
| 1 | Rename 15 directories | `mv` loop |
| 2 | Update all 15 Cargo.toml files | `patch` per file |
| 3 | Update `riv-macros` proc-macro sources (critical — hardcoded paths) | `patch` or `sed` |
| 4 | Update all Rust `use` paths & qualified paths workspace-wide | `find ... sed` |
| 5 | Update string-form serde attributes | `patch` |
| 6 | Update root README.md, AGENTS.md | `patch` |
| 7 | Update crate READMEs (6 files) | `patch` |
| 8 | Update `openspec/` design docs | `patch` per file |
| 9 | Update `linters.toml` comment | `patch` |
| 10 | Regenerate insta snapshots | `INSTA_UPDATE=new cargo test` |
| 11 | Regenerate trybuild stderr (if needed) | `TRYBUILD=overwrite cargo test` |
| 12 | Full workspace verification | `cargo check && cargo nextest` |

---

## Risks & Mitigations

1. **Proc-macro hardcoded paths** — HIGH risk. The `riv-macros` crate generates `::crate_name::` paths at compile time. If even one `::crb_cache::` path survives, the generated code will fail. Solution: triple-check with `grep -rn '::crb_' crates/` after sed.

2. **Ordering of compilation** — `riv-macros` must compile first since other crates use its generated code. The directory rename handles this since `riv-macros` is a leaf dependency.

3. **Binary name collision** — The workspace has no conflicting `riv` commands. `riv` (ex-crb-harness) is the main CLI, `riv-benchmark` is separate, `riv-webui` is the web server. No conflicts.

4. **Snapshot instability** — `.snap` files have `source: crates/crb_` annotation on line 2 that will auto-update when `INSTA_UPDATE=new` is used. This is expected and safe.

5. **`riv-stor` already uses `riv-` prefix** — It still depends on `crb-types` which becomes `riv-types`. This is handled in Phase 2 Cargo.toml updates.

6. **The `linters.toml` line 1 comment** — Low risk, cosmetic only.

---

## Files Changed Summary

- **15** crate directories renamed
- **16** Cargo.toml files modified (15 crates + root workspace — root workspace needs no changes since members are glob `["crates/*"]`)
- **~150+** Rust source files with `use crb_` paths
- **~20** insta snapshot files auto-regenerated
- **~8** doc/README files updated
- **~10** openspec design doc files updated

Total: ~200 files touched, but most changes are mechanical find-and-replace.
