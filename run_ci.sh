#!/usr/bin/env bash
set -euo pipefail
cargo run --bin crb-harness -- --ci 2>&1 | tee ci-output.log
exit "${PIPESTATUS[0]}"
