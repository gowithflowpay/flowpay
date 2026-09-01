#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.foundry/bin:$PATH"
set -a
source .env
set +a

for tool in cast curl jq; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done
: "${FLOWPAY_API_KEY:?FLOWPAY_API_KEY is required}"
: "${FLOWPAY_OPERATOR_PRIVATE_KEY:?FLOWPAY_OPERATOR_PRIVATE_KEY is required}"

payment_id="$(jq -er .id runtime/live-recovery-payment.json)"
tx_hash="$(jq -er '.transactionHash // .transaction_hash' runtime/live-recovery-base-transfer.json)"
wallet="$(cast wallet address --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY")"
claim_body="$(jq -nc \
  --arg payment_id "$payment_id" \
  --arg transaction_hash "$tx_hash" \
  --arg wallet "$wallet" \
  '{payment_id:$payment_id,transaction_hash:$transaction_hash,actual_chain:"base_sepolia",actual_asset:"USDC",originating_wallet:$wallet,recovery_destination:$wallet,explanation:"Sent 1 Base Sepolia USDC to an Ethereum Sepolia checkout address. Recover the verified wrong-chain deposit."}')"
created="$(curl -fsS http://127.0.0.1:8080/v1/claims \
  -H "x-flowpay-api-key: $FLOWPAY_API_KEY" \
  -H 'content-type: application/json' \
  -H "idempotency-key: live-claim-$tx_hash" \
  --data "$claim_body")"
claim_id="$(printf '%s' "$created" | jq -er .id)"
message="$(printf '%s' "$created" | jq -er .wallet_challenge.message)"

curl -fsS "http://127.0.0.1:8080/v1/claims/$claim_id/evidence" \
  -H "x-flowpay-api-key: $FLOWPAY_API_KEY" \
  -H 'content-type: application/json' \
  --data "$(jq -nc --arg tx "$tx_hash" '{evidence_type:"TX_HASH",text:$tx}')" >/dev/null

signature="$(cast wallet sign --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY" "$message")"
authorized="$(curl -fsS "http://127.0.0.1:8080/v1/claims/$claim_id/authorize" \
  -H "x-flowpay-api-key: $FLOWPAY_API_KEY" \
  -H 'content-type: application/json' \
  --data "$(jq -nc --arg signature "$signature" '{signature:$signature}')")"

printf '%s\n' "$created" >runtime/live-recovery-claim.json
echo "claim_id=$claim_id"
echo "wallet=$wallet"
echo "transaction_hash=$tx_hash"
echo "authorization=$(printf '%s' "$authorized" | jq -c .)"
