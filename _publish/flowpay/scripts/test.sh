#!/usr/bin/env bash
set -euo pipefail
cargo fmt --manifest-path backend/Cargo.toml --all -- --check
cargo clippy --manifest-path backend/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path backend/Cargo.toml --workspace
(cd contracts && forge test)
(cd apps/merchant && npm install --no-audit --no-fund && npm run build)
(cd apps/checkout && npm install --no-audit --no-fund && npm run build)
(cd sdk/node && npm install --no-audit --no-fund && npm test)
