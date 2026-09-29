#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
set -a
[[ -f "$ROOT/.env" ]] && source "$ROOT/.env"
source "$ROOT/runtime/bsc-testnet.env"
set +a

: "${FLOWPAY_OPERATOR_PRIVATE_KEY:?FLOWPAY_OPERATOR_PRIVATE_KEY is required}"
: "${FLOWPAY_OPERATOR_ADDRESS:?FLOWPAY_OPERATOR_ADDRESS is required}"
: "${BSC_TESTNET_RPC_URL:?BSC_TESTNET_RPC_URL is required}"

export PATH="$PATH:/root/.foundry/bin"
chain_id="$(cast chain-id --rpc-url "$BSC_TESTNET_RPC_URL")"
[[ "$chain_id" == "97" ]] || { echo "expected BSC testnet chain ID 97, got $chain_id" >&2; exit 1; }

deployment="$(cd "$ROOT/contracts" && forge create --broadcast --json \
  --rpc-url "$BSC_TESTNET_RPC_URL" \
  --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY" \
  src/TestToken.sol:TestToken \
  --constructor-args "FlowPay Test USDC" "USDC" 6)"

token="$(python3 - "$deployment" <<'PY'
import json, sys
obj = json.loads(sys.argv[1])
address = obj.get('deployedTo') or obj.get('deployed_to') or obj.get('address')
if not address:
    raise SystemExit('forge did not return the deployed token address')
print(address)
PY
)"

cast send "$token" "mint(address,uint256)" "$FLOWPAY_OPERATOR_ADDRESS" 1000000000 \
  --rpc-url "$BSC_TESTNET_RPC_URL" \
  --private-key "$FLOWPAY_OPERATOR_PRIVATE_KEY" >/dev/null

code="$(cast code "$token" --rpc-url "$BSC_TESTNET_RPC_URL")"
balance="$(cast call "$token" "balanceOf(address)(uint256)" "$FLOWPAY_OPERATOR_ADDRESS" --rpc-url "$BSC_TESTNET_RPC_URL")"
[[ "$code" != "0x" && "$balance" != "0" ]] || { echo "token deployment verification failed" >&2; exit 1; }

printf 'BSC_TESTNET_USDC_ADDRESS=%s\n' "$token"
printf 'BSC test USDC minted to operator: %s atomic units\n' "$balance"
