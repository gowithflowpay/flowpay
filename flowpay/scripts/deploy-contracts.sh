#!/usr/bin/env bash
set -euo pipefail
command -v forge >/dev/null || { echo "forge is required" >&2; exit 1; }
command -v cast >/dev/null || { echo "cast is required" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
mkdir -p runtime
DEPLOYER_PK="${FLOWPAY_LOCAL_DEPLOYER_PK:-0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80}"
CUSTOMER="${FLOWPAY_LOCAL_CUSTOMER:-0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC}"
SETTLEMENT="${FLOWPAY_LOCAL_SETTLEMENT:-0x90F79bf6EB2c4f870365E785982E1f101E93b906}"
OPERATOR="$(cast wallet address --private-key "$DEPLOYER_PK")"
create(){ local rpc="$1" contract="$2"; shift 2; (cd contracts && forge create --broadcast --json --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$contract" "$@") | jq -r '.deployedTo // .deployed_to // .address'; }
# Fresh local chains plus identical deployer/nonces make these addresses identical.
BASE_RPC=http://127.0.0.1:8545; BSC_RPC=http://127.0.0.1:9545
BASE_FACTORY=$(create "$BASE_RPC" src/FlowPayFactory.sol:FlowPayFactory --constructor-args "$OPERATOR")
BSC_FACTORY=$(create "$BSC_RPC" src/FlowPayFactory.sol:FlowPayFactory --constructor-args "$OPERATOR")
BASE_USDC=$(create "$BASE_RPC" src/TestToken.sol:TestToken --constructor-args 'Test USDC' 'USDC' 6)
BSC_USDC=$(create "$BSC_RPC" src/TestToken.sol:TestToken --constructor-args 'Test USDC' 'USDC' 6)
BASE_USDT=$(create "$BASE_RPC" src/TestToken.sol:TestToken --constructor-args 'Test USDT' 'USDT' 6)
BSC_USDT=$(create "$BSC_RPC" src/TestToken.sol:TestToken --constructor-args 'Test USDT' 'USDT' 6)
BASE_FAIL=$(create "$BASE_RPC" src/ToggleFailToken.sol:ToggleFailToken --constructor-args 'Simulation Fail Token' 'FAIL' 6)
BSC_FAIL=$(create "$BSC_RPC" src/ToggleFailToken.sol:ToggleFailToken --constructor-args 'Simulation Fail Token' 'FAIL' 6)
BASE_UNSUPPORTED=$(create "$BASE_RPC" src/TestToken.sol:TestToken --constructor-args 'Unsupported Token' 'UNSUP' 6)
BSC_UNSUPPORTED=$(create "$BSC_RPC" src/TestToken.sol:TestToken --constructor-args 'Unsupported Token' 'UNSUP' 6)
[[ "${BASE_FACTORY,,}" == "${BSC_FACTORY,,}" ]] || { echo "factory addresses differ; cross-chain invariant failed" >&2; exit 1; }
[[ "${BASE_USDC,,}" == "${BSC_USDC,,}" && "${BASE_USDT,,}" == "${BSC_USDT,,}" && "${BASE_FAIL,,}" == "${BSC_FAIL,,}" && "${BASE_UNSUPPORTED,,}" == "${BSC_UNSUPPORTED,,}" ]] || { echo "test token addresses unexpectedly differ" >&2; exit 1; }
mint_set(){ local rpc="$1" usdc="$2" usdt="$3" fail="$4" unsupported="$5";
  cast send --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$usdc" 'mint(address,uint256)' "$CUSTOMER" 1000000000 >/dev/null
  cast send --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$usdt" 'mint(address,uint256)' "$CUSTOMER" 1000000000 >/dev/null
  cast send --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$fail" 'mint(address,uint256)' "$CUSTOMER" 1000000000 >/dev/null
  cast send --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$unsupported" 'mint(address,uint256)' "$CUSTOMER" 1000000000 >/dev/null
}
mint_set "$BASE_RPC" "$BASE_USDC" "$BASE_USDT" "$BASE_FAIL" "$BASE_UNSUPPORTED"
mint_set "$BSC_RPC" "$BSC_USDC" "$BSC_USDT" "$BSC_FAIL" "$BSC_UNSUPPORTED"
PROXY_BYTECODE=$(cd contracts && forge inspect src/CheckoutProxy.sol:CheckoutProxy bytecode)
PROXY_HASH=$(cast keccak "$PROXY_BYTECODE")
FACTORY_CODE=$(cast code --rpc-url "$BASE_RPC" "$BASE_FACTORY")
FACTORY_HASH=$(cast keccak "$FACTORY_CODE")
cat > runtime/local.env <<ENV
FLOWPAY_FACTORY_ADDRESS=$BASE_FACTORY
FLOWPAY_PROXY_CREATION_CODE_HASH=$PROXY_HASH
FLOWPAY_FACTORY_RUNTIME_CODE_HASH=$FACTORY_HASH
FLOWPAY_OPERATOR_ADDRESS=$OPERATOR
FLOWPAY_FAUCET_ADDRESS=$OPERATOR
FLOWPAY_LOCAL_CUSTOMER=$CUSTOMER
FLOWPAY_LOCAL_SETTLEMENT=$SETTLEMENT
BASE_USDC_ADDRESS=$BASE_USDC
BASE_USDT_ADDRESS=$BASE_USDT
BSC_USDC_ADDRESS=$BSC_USDC
BSC_USDT_ADDRESS=$BSC_USDT
BASE_FAIL_ADDRESS=$BASE_FAIL
BSC_FAIL_ADDRESS=$BSC_FAIL
BASE_UNSUPPORTED_ADDRESS=$BASE_UNSUPPORTED
BSC_UNSUPPORTED_ADDRESS=$BSC_UNSUPPORTED
ENV
cat runtime/local.env
echo "Verified identical factory address across both local EVM chains: $BASE_FACTORY"
