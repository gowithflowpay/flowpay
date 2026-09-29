#!/bin/sh
set -eu
ROOT=/workspace
RUNTIME=/runtime
BASE_RPC=${BASE_RPC_URL:-http://base-anvil:8545}
BSC_RPC=${BSC_RPC_URL:-http://bsc-anvil:8545}
DEPLOYER_PK=${FLOWPAY_LOCAL_DEPLOYER_PK:?set FLOWPAY_LOCAL_DEPLOYER_PK for the disposable local chain}
CUSTOMER=${FLOWPAY_LOCAL_CUSTOMER:-0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC}
SETTLEMENT=${FLOWPAY_LOCAL_SETTLEMENT:-0x90F79bf6EB2c4f870365E785982E1f101E93b906}
OPERATOR=$(cast wallet address --private-key "$DEPLOYER_PK")
wait_rpc(){ url="$1"; i=0; until cast chain-id --rpc-url "$url" >/dev/null 2>&1; do i=$((i+1)); [ "$i" -gt 90 ] && exit 1; sleep 1; done; }
wait_rpc "$BASE_RPC"; wait_rpc "$BSC_RPC"
create(){ rpc="$1"; contract="$2"; shift 2; (cd "$ROOT/contracts" && forge create --broadcast --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$contract" "$@") | awk '/Deployed to:/{print $3}' | tail -1; }
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
[ "$(printf '%s' "$BASE_FACTORY" | tr 'A-Z' 'a-z')" = "$(printf '%s' "$BSC_FACTORY" | tr 'A-Z' 'a-z')" ] || { echo 'factory addresses differ' >&2; exit 1; }
mint_set(){ rpc="$1"; usdc="$2"; usdt="$3"; fail="$4"; unsupported="$5";
  cast send --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$usdc" 'mint(address,uint256)' "$CUSTOMER" 1000000000 >/dev/null
  cast send --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$usdt" 'mint(address,uint256)' "$CUSTOMER" 1000000000 >/dev/null
  cast send --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$fail" 'mint(address,uint256)' "$CUSTOMER" 1000000000 >/dev/null
  cast send --rpc-url "$rpc" --private-key "$DEPLOYER_PK" "$unsupported" 'mint(address,uint256)' "$CUSTOMER" 1000000000 >/dev/null
}
mint_set "$BASE_RPC" "$BASE_USDC" "$BASE_USDT" "$BASE_FAIL" "$BASE_UNSUPPORTED"
mint_set "$BSC_RPC" "$BSC_USDC" "$BSC_USDT" "$BSC_FAIL" "$BSC_UNSUPPORTED"
PROXY_BYTECODE=$(cd "$ROOT/contracts" && forge inspect src/CheckoutProxy.sol:CheckoutProxy bytecode)
PROXY_HASH=$(cast keccak "$PROXY_BYTECODE")
FACTORY_CODE=$(cast code --rpc-url "$BASE_RPC" "$BASE_FACTORY")
FACTORY_HASH=$(cast keccak "$FACTORY_CODE")
mkdir -p "$RUNTIME"
cat > "$RUNTIME/local.env" <<ENV
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
echo "verified identical FlowPay factory on both local chains: $BASE_FACTORY"
