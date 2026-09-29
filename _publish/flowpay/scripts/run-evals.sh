#!/usr/bin/env bash
set -euo pipefail
node evals/runner/run.mjs
if command -v cargo >/dev/null; then cargo run -q --manifest-path evals/runner-rust/Cargo.toml -- --result evals/results/agent.json; fi
