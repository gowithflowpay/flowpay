#!/usr/bin/env bash
set -euo pipefail
[[ -f /root/.cargo/env ]] && source /root/.cargo/env
BSC_OVERRIDE="${FLOWPAY_BSC_RPC_OVERRIDE:-}"
ENV_OVERRIDE="${FLOWPAY_ENV_OVERRIDE:-}"
set -a
[[ -f .env ]] && source .env
[[ -n "$ENV_OVERRIDE" ]] && FLOWPAY_ENV="$ENV_OVERRIDE"
if [[ "${FLOWPAY_ENV:-local}" == "local" ]]; then
  [[ -f runtime/local.env ]] || { echo "runtime/local.env missing; run scripts/reset-local.sh first" >&2; exit 1; }
  source runtime/local.env
elif [[ "${FLOWPAY_ENV:-local}" == "testnet" ]]; then
  for metadata in runtime/base-sepolia.env runtime/ethereum-sepolia.env runtime/arbitrum-sepolia.env runtime/bsc-testnet.env; do
    [[ -f "$metadata" ]] && source "$metadata"
  done
fi
set +a
[[ -n "$BSC_OVERRIDE" ]] && export BSC_RPC_URL="$BSC_OVERRIDE"
export FLOWPAY_WEBHOOK_ENCRYPTION_KEY="${FLOWPAY_WEBHOOK_ENCRYPTION_KEY:-0000000000000000000000000000000000000000000000000000000000000000}"
export FLOWPAY_API_KEY_HASH_PEPPER="${FLOWPAY_API_KEY_HASH_PEPPER:-local-dev-pepper-change-me}"
export RABBITMQ_URL="${RABBITMQ_URL:-amqp://guest:guest@127.0.0.1:5672/%2f}"
MODE="${1:-${FLOWPAY_AGENT_MODE:-model}}"; export FLOWPAY_AGENT_MODE="$MODE"
BIN=backend/target/debug/flowpay-worker
if [[ -x "$BIN" ]]; then exec "$BIN"; fi
exec cargo run --manifest-path backend/Cargo.toml -q -p flowpay-worker
