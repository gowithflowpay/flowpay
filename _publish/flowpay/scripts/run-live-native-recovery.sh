#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.foundry/bin:$PATH"
set -a
source .env
set +a

for tool in cast curl jq psql; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done
: "${FLOWPAY_API_KEY:?FLOWPAY_API_KEY is required}"
: "${FLOWPAY_OPERATOR_PRIVATE_KEY:?FLOWPAY_OPERATOR_PRIVATE_KEY is required}"
: "${BASE_SEPOLIA_RPC_URL:?BASE_SEPOLIA_RPC_URL is required}"
: "${FLOWPAY_TESTNET_SETTLEMENT_ADDRESS:?FLOWPAY_TESTNET_SETTLEMENT_ADDRESS is required}"

api="${FLOWPAY_API_URL:-http://127.0.0.1:8080}"
sender="$(cast wallet address --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY")"
refund="$FLOWPAY_TESTNET_SETTLEMENT_ADDRESS"
curl -fsS "$api/health" >/dev/null
./scripts/seed-testnet-assets.sh >/dev/null

before_sender="$(cast balance --rpc-url "$BASE_SEPOLIA_RPC_URL" "$sender")"
before_refund="$(cast balance --rpc-url "$BASE_SEPOLIA_RPC_URL" "$refund")"
[[ "$before_sender" -ge 200000000000000 ]] || {
  echo "insufficient Base Sepolia ETH for deposit and recovery gas" >&2
  exit 3
}

run_id="${FLOWPAY_RECOVERY_RUN_ID:-$(date -u +%Y%m%d%H%M%S)}"
payment_file="runtime/live-native-recovery-payment-$run_id.json"
transfer_file="runtime/live-native-recovery-transfer-$run_id.json"
claim_file="runtime/live-native-recovery-claim-$run_id.json"
final_file="runtime/live-native-recovery-final-$run_id.json"

if [[ -f "$payment_file" ]]; then
  payment="$(<"$payment_file")"
else
  payment="$(curl -fsS "$api/v1/payments" \
    -H "x-flowpay-api-key: $FLOWPAY_API_KEY" \
    -H 'content-type: application/json' \
    -H "idempotency-key: live-native-payment-$run_id" \
    --data "$(jq -nc '{amount:"0.0001",asset:"ETH",chain:"ethereum_sepolia",reference:"LIVE_BASE_NATIVE_RECOVERY",expires_in_seconds:3600}')")"
  printf '%s\n' "$payment" >"$payment_file"
fi
payment_id="$(jq -er .id <<<"$payment")"
checkout_address="$(jq -er .address <<<"$payment")"

if [[ -f "$transfer_file" ]]; then
  transfer="$(<"$transfer_file")"
else
  transfer="$(cast send \
    --rpc-url "$BASE_SEPOLIA_RPC_URL" \
    --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY" \
    --value 0.0001ether \
    --json \
    "$checkout_address")"
  printf '%s\n' "$transfer" >"$transfer_file"
fi
tx_hash="$(jq -er '.transactionHash // .transaction_hash' <<<"$transfer")"
status="$(cast receipt --rpc-url "$BASE_SEPOLIA_RPC_URL" "$tx_hash" --json | jq -er .status)"
[[ "$status" == "1" || "$status" == "0x1" ]] || {
  echo "deposit transaction failed: $tx_hash" >&2
  exit 4
}

if [[ -f "$claim_file" ]]; then
  claim="$(<"$claim_file")"
  claim_id="$(jq -er .id <<<"$claim")"
  current_state="$(curl -fsS "$api/v1/claims/$claim_id" \
    -H "x-flowpay-api-key: $FLOWPAY_API_KEY" | jq -er .status)"
  if [[ "$current_state" != "RECOVERED" ]]; then
    curl -fsS -X POST "$api/v1/claims/$claim_id/retry" \
      -H "x-flowpay-api-key: $FLOWPAY_API_KEY" >/dev/null
  fi
else
  claim="$(curl -fsS "$api/v1/claims" \
    -H "x-flowpay-api-key: $FLOWPAY_API_KEY" \
    -H 'content-type: application/json' \
    -H "idempotency-key: live-native-claim-$tx_hash" \
    --data "$(jq -nc \
      --arg payment_id "$payment_id" \
      --arg transaction_hash "$tx_hash" \
      --arg sender "$sender" \
      --arg refund "$refund" \
      '{payment_id:$payment_id,transaction_hash:$transaction_hash,actual_chain:"base_sepolia",actual_asset:"ETH",originating_wallet:$sender,recovery_destination:$refund,explanation:"Sent native ETH on Base Sepolia to an Ethereum Sepolia checkout address."}')")"
  printf '%s\n' "$claim" >"$claim_file"
  claim_id="$(jq -er .id <<<"$claim")"
  message="$(jq -er .wallet_challenge.message <<<"$claim")"
  signature="$(cast wallet sign --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY" "$message")"

  curl -fsS "$api/v1/claims/$claim_id/evidence" \
    -H "x-flowpay-api-key: $FLOWPAY_API_KEY" \
    -H 'content-type: application/json' \
    --data "$(jq -nc --arg tx "$tx_hash" '{evidence_type:"TX_HASH",text:$tx}')" >/dev/null
  curl -fsS "$api/v1/claims/$claim_id/authorize" \
    -H "x-flowpay-api-key: $FLOWPAY_API_KEY" \
    -H 'content-type: application/json' \
    --data "$(jq -nc --arg signature "$signature" '{signature:$signature}')" >/dev/null
fi

final=""
for _ in $(seq 1 120); do
  final="$(curl -fsS "$api/v1/claims/$claim_id" -H "x-flowpay-api-key: $FLOWPAY_API_KEY")"
  state="$(jq -er .status <<<"$final")"
  if [[ "$state" == "RECOVERED" ]]; then
    break
  fi
  if [[ "$state" == "ESCALATED" || "$state" == "NOT_RECOVERABLE" || "$state" == "REJECTED" ]]; then
    printf '%s\n' "$final" >"$final_file"
    echo "claim stopped in $state" >&2
    exit 5
  fi
  sleep 3
done
printf '%s\n' "$final" >"$final_file"
[[ "$(jq -er .status <<<"$final")" == "RECOVERED" ]] || {
  echo "claim did not recover before timeout" >&2
  exit 6
}

recovery_hash="$(jq -er .recovery.recovery_tx <<<"$final")"
recovery_status="$(cast receipt --rpc-url "$BASE_SEPOLIA_RPC_URL" "$recovery_hash" status)"
after_refund="$(cast balance --rpc-url "$BASE_SEPOLIA_RPC_URL" "$refund")"
checkout_after="$(cast balance --rpc-url "$BASE_SEPOLIA_RPC_URL" "$checkout_address")"
[[ "$recovery_status" == "1" || "$recovery_status" == "0x1" ]]
[[ "$checkout_after" == "0" ]]

jq -nc \
  --arg payment_id "$payment_id" \
  --arg checkout_url "$(jq -er .checkout_url <<<"$payment")" \
  --arg checkout_address "$checkout_address" \
  --arg deposit_tx "$tx_hash" \
  --arg claim_id "$claim_id" \
  --arg recovery_tx "$recovery_hash" \
  --arg refund_address "$refund" \
  --arg refund_before "$before_refund" \
  --arg refund_after "$after_refund" \
  --arg checkout_after "$checkout_after" \
  '{payment_id:$payment_id,checkout_url:$checkout_url,checkout_address:$checkout_address,deposit_tx:$deposit_tx,claim_id:$claim_id,claim_status:"RECOVERED",recovery_tx:$recovery_tx,recovery_receipt:"SUCCESS",refund_address:$refund_address,refund_balance_before:$refund_before,refund_balance_after:$refund_after,checkout_balance_after:$checkout_after}'
