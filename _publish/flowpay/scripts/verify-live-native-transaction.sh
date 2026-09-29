#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.foundry/bin:$PATH"
set -a
source .env
set +a
tx_hash="${1:?transaction hash required}"
checkout="${2:?checkout address required}"
cast receipt --rpc-url "$BASE_SEPOLIA_RPC_URL" "$tx_hash" --json | jq -c '{transactionHash,status,blockNumber,blockHash,gasUsed,effectiveGasPrice,from,to,logs}'
jq -nc \
  --arg checkout "$checkout" \
  --arg checkout_balance "$(cast balance --rpc-url "$BASE_SEPOLIA_RPC_URL" "$checkout")" \
  --arg destination "$FLOWPAY_TESTNET_SETTLEMENT_ADDRESS" \
  --arg destination_balance "$(cast balance --rpc-url "$BASE_SEPOLIA_RPC_URL" "$FLOWPAY_TESTNET_SETTLEMENT_ADDRESS")" \
  '{checkout:$checkout,checkout_balance:$checkout_balance,destination:$destination,destination_balance:$destination_balance}'
