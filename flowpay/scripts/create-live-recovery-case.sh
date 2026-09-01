#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
set -a
source .env
set +a

for tool in curl jq; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done
: "${FLOWPAY_API_KEY:?FLOWPAY_API_KEY is required}"

response="$(curl -fsS http://127.0.0.1:8080/v1/payments \
  -H "x-flowpay-api-key: $FLOWPAY_API_KEY" \
  -H 'content-type: application/json' \
  -H "idempotency-key: live-cross-chain-$(date +%s)" \
  --data '{"amount":"1","asset":"USDC","chain":"ethereum_sepolia","reference":"LIVE_BASE_USDC_RECOVERY","expires_in_seconds":86400,"overpayment_policy":"REQUIRE_REVIEW"}')"

printf '%s\n' "$response" | jq .
printf '%s\n' "$response" >runtime/live-recovery-payment.json
