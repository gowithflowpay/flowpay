#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.foundry/bin:$PATH"
set -a
source .env
set +a

for tool in cast jq; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done
: "${FLOWPAY_OPERATOR_PRIVATE_KEY:?FLOWPAY_OPERATOR_PRIVATE_KEY is required}"
: "${BASE_SEPOLIA_RPC_URL:?BASE_SEPOLIA_RPC_URL is required}"
: "${BASE_SEPOLIA_USDC_ADDRESS:?BASE_SEPOLIA_USDC_ADDRESS is required}"

payment_address="$(jq -er .address runtime/live-recovery-payment.json)"
receipt="$(cast send --json \
  --rpc-url "$BASE_SEPOLIA_RPC_URL" \
  --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY" \
  "$BASE_SEPOLIA_USDC_ADDRESS" \
  'transfer(address,uint256)' "$payment_address" 1000000)"

printf '%s\n' "$receipt" >runtime/live-recovery-base-transfer.json
echo "transaction_hash=$(printf '%s' "$receipt" | jq -er '.transactionHash // .transaction_hash')"
echo "block_number=$(printf '%s' "$receipt" | jq -er '.blockNumber // .block_number')"
echo "status=$(printf '%s' "$receipt" | jq -er '.status')"
