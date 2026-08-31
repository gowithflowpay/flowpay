#!/usr/bin/env bash
set -euo pipefail
for tool in cargo psql anvil forge cast node curl; do command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }; done
MODE="${1:-all}"
[[ "$MODE" =~ ^(baseline|model|all)$ ]] || { echo "usage: $0 [baseline|model|all]" >&2; exit 2; }
set -a; [[ -f .env ]] && source .env; set +a
if [[ "$MODE" != "baseline" && "${FLOWPAY_MODEL_PROVIDER:-ollama}" == "ollama" ]]; then
  ollama_ready=0
  for _ in 1 2 3; do
    if curl -fsS --max-time 180 "${FLOWPAY_MODEL_ENDPOINT:-http://127.0.0.1:11434/api/chat}" -o /dev/null -X POST -H 'content-type: application/json' --data "{\"model\":\"${FLOWPAY_AGENT_MODEL:-qwen2.5-coder:7b}\",\"messages\":[{\"role\":\"user\",\"content\":\"Reply OK\"}],\"stream\":false}"; then
      ollama_ready=1
      break
    fi
    sleep 2
  done
  [[ "$ollama_ready" == 1 ]] || { echo "Ollama model endpoint is unavailable" >&2; exit 2; }
fi
mkdir -p runtime evals/results/e2e evals/trajectories/e2e
cargo build --manifest-path backend/Cargo.toml -q -p flowpay-api-server -p flowpay-worker -p flowpay-event-relay
API_PID=""; WORKER_PID=""; RELAY_PID=""; PROXY_PID=""; SINK_PID=""
cleanup(){ for p in "$API_PID" "$WORKER_PID" "$RELAY_PID" "$PROXY_PID" "$SINK_PID"; do [[ -n "$p" ]] && kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true; }
trap cleanup EXIT
run_mode(){
  local mode="$1"; cleanup; API_PID=""; WORKER_PID=""; RELAY_PID=""; PROXY_PID=""; SINK_PID=""
  ./scripts/reset-local.sh
  node infra/rpc-proxy/index.mjs >"runtime/rpc-proxy-${mode}.log" 2>&1 & PROXY_PID=$!
  node evals/e2e/webhook-sink.mjs >"runtime/webhook-sink-${mode}.log" 2>&1 & SINK_PID=$!
  for _ in $(seq 1 40); do curl -fsS http://127.0.0.1:9546/__status >/dev/null 2>&1 && curl -fsS http://127.0.0.1:9555/__status >/dev/null 2>&1 && break; sleep .1; done
  ./scripts/run-event-relay.sh >"runtime/event-relay-${mode}.log" 2>&1 & RELAY_PID=$!
  FLOWPAY_ENV_OVERRIDE=local FLOWPAY_BSC_RPC_OVERRIDE=http://127.0.0.1:9546 FLOWPAY_AGENT_MODE="$mode" ./scripts/run-worker.sh "$mode" >"runtime/worker-${mode}.log" 2>&1 & WORKER_PID=$!
  FLOWPAY_ENV_OVERRIDE=local FLOWPAY_BSC_RPC_OVERRIDE=http://127.0.0.1:9546 FLOWPAY_AGENT_MODE="$mode" ./scripts/run-api.sh "$mode" >"runtime/api-${mode}.log" 2>&1 & API_PID=$!
  local ok=0
  for _ in $(seq 1 120); do
    if curl -fsS http://127.0.0.1:8080/health >/dev/null 2>&1; then ok=1; break; fi
    if ! kill -0 "$API_PID" 2>/dev/null; then echo "API exited; see runtime/api-${mode}.log" >&2; tail -80 "runtime/api-${mode}.log" >&2 || true; exit 1; fi
    sleep .25
  done
  [[ "$ok" == 1 ]] || { echo "API health check timed out" >&2; exit 1; }
  node evals/e2e/run.mjs --mode "$mode" --output "evals/results/e2e/${mode}.json"
}
if [[ "$MODE" == "baseline" || "$MODE" == "all" ]]; then run_mode baseline; fi
if [[ "$MODE" == "model" || "$MODE" == "all" ]]; then run_mode model; fi
if [[ "$MODE" == "all" ]]; then node evals/e2e/compare.mjs; fi
