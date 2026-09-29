#!/usr/bin/env bash
set -euo pipefail
for tool in cargo npm; do command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }; done
[[ -f runtime/local.env ]] || ./scripts/bootstrap-local.sh
mkdir -p runtime
pids=()
cleanup(){ for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done; wait 2>/dev/null || true; }
trap cleanup EXIT INT TERM
./scripts/run-event-relay.sh >runtime/event-relay.log 2>&1 & pids+=("$!")
./scripts/run-worker.sh deterministic >runtime/worker.log 2>&1 & pids+=("$!")
./scripts/run-api.sh deterministic >runtime/api.log 2>&1 & pids+=("$!")
(cd apps/merchant && npm run dev) >runtime/merchant.log 2>&1 & pids+=("$!")
(cd apps/checkout && npm run dev) >runtime/checkout.log 2>&1 & pids+=("$!")
printf '%s\n' 'FlowPay started. Logs are under runtime/. Press Ctrl-C to stop.'
wait
