#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.foundry/bin:$PATH"
set -a
for file in runtime/ethereum-sepolia.env runtime/base-sepolia.env; do
  [[ -f "$file" ]] && source "$file"
done
source .env
set +a

for tool in cast curl psql; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done

wallet="$(cast wallet address --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY")"
base_token="${BASE_SEPOLIA_USDC_ADDRESS:?BASE_SEPOLIA_USDC_ADDRESS is required}"
eth_factory="${ETHEREUM_SEPOLIA_FACTORY_ADDRESS:?ETHEREUM_SEPOLIA_FACTORY_ADDRESS is required}"
base_factory="${BASE_SEPOLIA_FACTORY_ADDRESS:?BASE_SEPOLIA_FACTORY_ADDRESS is required}"

echo "wallet=$wallet"
echo "base_native_wei=$(cast balance --rpc-url "$BASE_SEPOLIA_RPC_URL" "$wallet")"
echo "base_usdc_atomic=$(cast call --rpc-url "$BASE_SEPOLIA_RPC_URL" "$base_token" 'balanceOf(address)(uint256)' "$wallet")"
ethereum_factory_code="$(cast code --rpc-url "$ETHEREUM_SEPOLIA_RPC_URL" "$eth_factory")"
base_factory_code="$(cast code --rpc-url "$BASE_SEPOLIA_RPC_URL" "$base_factory")"
echo "ethereum_factory_code_bytes=$(( (${#ethereum_factory_code} - 2) / 2 ))"
echo "base_factory_code_bytes=$(( (${#base_factory_code} - 2) / 2 ))"
echo "ollama_http=$(curl -sS -o /dev/null -w '%{http_code}' --max-time 5 "${FLOWPAY_OPENAI_BASE_URL:-http://127.0.0.1:11434/api/chat}")"
echo "database_assets:"
psql "$DATABASE_URL" -AtF '|' -c "SELECT chain,symbol,token_contract,decimals,enabled FROM chain_assets WHERE chain IN ('ethereum_sepolia','base_sepolia') AND upper(symbol)='USDC' ORDER BY chain"
