#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.foundry/bin:$PATH"
set -a
source .env
set +a

: "${BASE_SEPOLIA_RPC_URL:?BASE_SEPOLIA_RPC_URL is required}"
: "${BASE_SEPOLIA_USDC_ADDRESS:?BASE_SEPOLIA_USDC_ADDRESS is required}"
: "${FLOWPAY_OPERATOR_PRIVATE_KEY:?FLOWPAY_OPERATOR_PRIVATE_KEY is required}"

checkout_address="${1:?checkout address is required}"
amount_atomic="${2:?USDC amount in atomic units is required}"
[[ "$checkout_address" =~ ^0x[0-9a-fA-F]{40}$ ]] || { echo "invalid checkout address" >&2; exit 2; }
[[ "$amount_atomic" =~ ^[1-9][0-9]*$ ]] || { echo "amount must be a positive integer" >&2; exit 2; }

sender="$(cast wallet address --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY")"
token_balance="$(cast call --rpc-url "$BASE_SEPOLIA_RPC_URL" "$BASE_SEPOLIA_USDC_ADDRESS" 'balanceOf(address)(uint256)' "$sender")"
token_balance="${token_balance%% *}"
(( token_balance >= amount_atomic )) || { echo "insufficient test USDC balance" >&2; exit 1; }

receipt="$(cast send --json \
  --rpc-url "$BASE_SEPOLIA_RPC_URL" \
  --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY" \
  "$BASE_SEPOLIA_USDC_ADDRESS" \
  'transfer(address,uint256)' "$checkout_address" "$amount_atomic")"

printf '%s\n' "$receipt" >runtime/checkout-usdc-transfer.json
echo "transaction_hash=$(printf '%s' "$receipt" | jq -er '.transactionHash // .transaction_hash')"
echo "block_number=$(printf '%s' "$receipt" | jq -er '.blockNumber // .block_number')"
echo "status=$(printf '%s' "$receipt" | jq -er '.status')"
