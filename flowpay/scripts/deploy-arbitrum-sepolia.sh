#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEY_FILE="${FLOWPAY_DEPLOYER_KEY_FILE:-$ROOT/../deployer.txt}"
RPC_URL="${ARBITRUM_SEPOLIA_RPC_URL:-https://arbitrum-sepolia-rpc.publicnode.com}"
RUNTIME_DIR="${FLOWPAY_RUNTIME_DIR:-$ROOT/runtime}"

command -v forge >/dev/null || { echo "forge is required" >&2; exit 1; }
command -v cast >/dev/null || { echo "cast is required" >&2; exit 1; }
[[ -r "$KEY_FILE" ]] || { echo "deployer key file is not readable: $KEY_FILE" >&2; exit 1; }

DEPLOYER_PK="$(python3 - "$KEY_FILE" <<'PY'
import re, sys
from pathlib import Path
text = Path(sys.argv[1]).read_text().strip()
match = re.search(r'(?i)(?:0x)?[0-9a-f]{64}', text)
if not match:
    raise SystemExit('deployer.txt does not contain a 32-byte hex private key')
print('0x' + match.group(0).removeprefix('0x'))
PY
)"
CHAIN_ID="$(cast chain-id --rpc-url "$RPC_URL")"
[[ "$CHAIN_ID" == "421614" ]] || { echo "refusing deployment: RPC chain ID is $CHAIN_ID, expected 421614" >&2; exit 1; }

OPERATOR="$(cast wallet address --private-key "$DEPLOYER_PK")"
mkdir -p "$RUNTIME_DIR"
FACTORY_JSON="$(cd "$ROOT/contracts" && forge create --broadcast --json --rpc-url "$RPC_URL" --private-key "$DEPLOYER_PK" src/FlowPayFactory.sol:FlowPayFactory --constructor-args "$OPERATOR")"
FACTORY="$(python3 - "$FACTORY_JSON" <<'PY'
import json, sys
obj = json.loads(sys.argv[1])
address = obj.get('deployedTo') or obj.get('deployed_to') or obj.get('address')
if not address: raise SystemExit('forge did not return a deployed factory address')
print(address)
PY
)"
PROXY_BYTECODE="$(cd "$ROOT/contracts" && forge inspect src/CheckoutProxy.sol:CheckoutProxy bytecode)"
PROXY_HASH="$(cast keccak "$PROXY_BYTECODE")"
FACTORY_CODE="$(cast code --rpc-url "$RPC_URL" "$FACTORY")"
FACTORY_HASH="$(cast keccak "$FACTORY_CODE")"
umask 077
cat > "$RUNTIME_DIR/arbitrum-sepolia.env" <<ENV
ARBITRUM_SEPOLIA_RPC_URL=$RPC_URL
ARBITRUM_SEPOLIA_CHAIN_ID=421614
ARBITRUM_SEPOLIA_FACTORY_ADDRESS=$FACTORY
ARBITRUM_SEPOLIA_OPERATOR_ADDRESS=$OPERATOR
ARBITRUM_SEPOLIA_PROXY_CREATION_CODE_HASH=$PROXY_HASH
ARBITRUM_SEPOLIA_FACTORY_RUNTIME_CODE_HASH=$FACTORY_HASH
ENV
printf 'Arbitrum Sepolia factory deployed: %s\n' "$FACTORY"
printf 'Operator address: %s\n' "$OPERATOR"
printf 'Metadata written to: %s\n' "$RUNTIME_DIR/arbitrum-sepolia.env"
