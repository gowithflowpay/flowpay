#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
set -a
source .env
set +a
: "${FLOWPAY_API_KEY:?FLOWPAY_API_KEY is required}"
claim_id="$(jq -er .id runtime/live-recovery-claim.json)"
curl -fsS "http://127.0.0.1:8080/v1/claims/$claim_id" \
  -H "x-flowpay-api-key: $FLOWPAY_API_KEY" | jq '{id,status,investigation,recovery,agent}'
