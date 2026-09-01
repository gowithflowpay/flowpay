#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
set -a
source .env
set +a
: "${FLOWPAY_API_KEY:?FLOWPAY_API_KEY is required}"

payment_id="$(jq -er .id runtime/live-recovery-payment.json)"
curl -fsS "http://127.0.0.1:8080/v1/payments/$payment_id" \
  -H "x-flowpay-api-key: $FLOWPAY_API_KEY" | jq .
curl -fsS "http://127.0.0.1:8080/v1/payments/$payment_id/deposits" \
  -H "x-flowpay-api-key: $FLOWPAY_API_KEY" | jq .
psql "$DATABASE_URL" -AtF '|' -c "SELECT c.chain,coalesce(c.last_scanned_block::text,'null'),coalesce(c.last_scanned_block_hash,'null') FROM payment_monitor_cursors c JOIN payments p ON p.id=c.payment_id WHERE p.public_id='$payment_id' ORDER BY c.chain" 2>/dev/null || true
