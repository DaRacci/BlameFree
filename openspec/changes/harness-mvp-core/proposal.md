# Change: Workspace Scaffold & Core Harness

## Intent
Create the Cargo workspace skeleton with all crate stubs, set up inter-crate dependencies, and implement the main binary crate (crb-harness) that orchestrates the full benchmark pipeline.

## Scope
Cargo workspace with 8 member crates (1 binary + 7 library). The main binary assembles all components. Golden comments datasets copied.

## Approach
Cargo workspace at root, crates/ subdirectory, each crate independently compilable. crb-aggregator and crb-auditor have both lib.rs (library) and main.rs (CLI entrypoint). crb-harness depends on all 7 library crates.
