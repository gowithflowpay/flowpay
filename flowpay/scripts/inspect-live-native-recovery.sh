#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
set -a
source .env
set +a
run_id="${1:?run id required}"
claim_id="$(jq -er .id "runtime/live-native-recovery-claim-$run_id.json")"
if [[ "${2:-}" == "--retry" ]]; then
  curl -sS -X POST "${FLOWPAY_API_URL:-http://127.0.0.1:8080}/v1/claims/$claim_id/retry" \
    -H "x-flowpay-api-key: $FLOWPAY_API_KEY"
  echo
fi
curl -fsS "${FLOWPAY_API_URL:-http://127.0.0.1:8080}/v1/claims/$claim_id" \
  -H "x-flowpay-api-key: $FLOWPAY_API_KEY" | jq -c '{id,status,investigation,agent,recovery}'
