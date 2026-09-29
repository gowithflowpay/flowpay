#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

bash ./scripts/start-backend.sh &
BACKEND_PID=$!

cleanup() {
  kill "$BACKEND_PID" "$MERCHANT_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

bash ./scripts/start-merchant.sh &
MERCHANT_PID=$!

wait "$BACKEND_PID" "$MERCHANT_PID"
