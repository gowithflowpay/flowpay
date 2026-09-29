#!/usr/bin/env bash
set -euo pipefail
command -v anvil >/dev/null || { echo "anvil is required (install Foundry)" >&2; exit 1; }
mkdir -p runtime
MNEMONIC="${FLOWPAY_LOCAL_MNEMONIC:-test test test test test test test test test test test junk}"
stop_pid(){ local file="$1"; if [[ -f "$file" ]]; then kill "$(cat "$file")" 2>/dev/null || true; fi; }
stop_pid runtime/base-anvil.pid; stop_pid runtime/bsc-anvil.pid
anvil --silent --host 127.0.0.1 --port 8545 --chain-id 31337 --mnemonic "$MNEMONIC" >runtime/base-anvil.log 2>&1 & echo $! > runtime/base-anvil.pid
anvil --silent --host 127.0.0.1 --port 9545 --chain-id 31338 --mnemonic "$MNEMONIC" >runtime/bsc-anvil.log 2>&1 & echo $! > runtime/bsc-anvil.pid
for url in http://127.0.0.1:8545 http://127.0.0.1:9545; do
  for _ in $(seq 1 50); do curl -fsS -X POST -H 'content-type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}' "$url" >/dev/null && break; sleep .1; done
done
echo "Base-local: 31337 @ http://127.0.0.1:8545"
echo "BSC-local:  31338 @ http://127.0.0.1:9545"
