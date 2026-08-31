#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
mkdir -p runtime

set -a
[[ -f .env ]] && source .env
set +a

MODE="${1:-${FLOWPAY_AGENT_MODE:-model}}"
if [[ "$MODE" != "model" && "$MODE" != "deterministic" && "$MODE" != "baseline" ]]; then
  echo "mode must be model, deterministic, or baseline" >&2
  exit 2
fi

for tool in cargo curl node npm psql; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done

ensure_service() {
  local unit="$1"
  if systemctl is-active --quiet "$unit"; then return; fi
  if sudo -n systemctl start "$unit" 2>/dev/null; then return; fi
  echo "$unit is not running; start it once with: sudo systemctl start $unit" >&2
  exit 1
}

ensure_service postgresql@16-main
ensure_service rabbitmq-server

if ! nc -z 127.0.0.1 9092 2>/dev/null; then
  command -v docker >/dev/null || { echo "Kafka is unavailable and docker is not installed" >&2; exit 1; }
  if docker ps -a --format '{{.Names}}' | grep -qx flowpay-kafka; then
    docker start flowpay-kafka >/dev/null
  else
    docker run -d --name flowpay-kafka --restart unless-stopped \
      -p 9092:9092 docker.redpanda.com/redpandadata/redpanda:latest \
      redpanda start --overprovisioned --smp 1 --memory 512M --reserve-memory 0M \
      --node-id 0 --check=false \
      --kafka-addr 0.0.0.0:9092 \
      --advertise-kafka-addr 127.0.0.1:9092 >/dev/null
  fi
fi

for _ in {1..30}; do nc -z 127.0.0.1 9092 2>/dev/null && break; sleep 1; done
nc -z 127.0.0.1 9092 2>/dev/null || { echo "Kafka did not become ready" >&2; exit 1; }

if [[ "$(psql "$DATABASE_URL" -Atc "select to_regclass('public.payments')" 2>/dev/null || true)" != "payments" ]]; then
  ./scripts/migrate.sh
fi
if [[ "${FLOWPAY_ENV:-local}" == "testnet" ]]; then
  ./scripts/seed-testnet-assets.sh
else
  ./scripts/seed.sh
fi
node tools/verify-create3.mjs

cargo build --manifest-path backend/Cargo.toml \
  -p flowpay-api-server -p flowpay-worker -p flowpay-event-relay
for app in apps/merchant apps/checkout; do
  [[ -d "$app/node_modules" ]] || (cd "$app" && npm install)
done

pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

./scripts/run-event-relay.sh >runtime/event-relay.log 2>&1 & pids+=("$!")
./scripts/run-worker.sh "$MODE" >runtime/worker.log 2>&1 & pids+=("$!")
./scripts/run-api.sh "$MODE" >runtime/api.log 2>&1 & pids+=("$!")
(cd apps/merchant && npm run dev) >runtime/merchant.log 2>&1 & pids+=("$!")
(cd apps/checkout && npm run dev) >runtime/checkout.log 2>&1 & pids+=("$!")

if command -v ngrok >/dev/null && [[ "${FLOWPAY_NGROK_ENABLED:-true}" == "true" ]]; then
  ngrok http 8080 --log stdout >runtime/ngrok.log 2>&1 & pids+=("$!")
fi

for _ in {1..60}; do
  curl -fsS http://127.0.0.1:8080/health >/dev/null 2>&1 && break
  sleep 1
done
curl -fsS http://127.0.0.1:8080/health >/dev/null

echo "FlowPay is ready"
echo "Merchant: http://127.0.0.1:3000"
echo "Checkout: http://127.0.0.1:3001"
echo "API:      http://127.0.0.1:8080"
if curl -fsS http://127.0.0.1:4040/api/tunnels >/dev/null 2>&1; then
  node -e 'fetch("http://127.0.0.1:4040/api/tunnels").then(r=>r.json()).then(v=>console.log(`Webhook: ${v.tunnels[0].public_url}/v1/providers/alchemy/webhook`))'
fi
echo "Logs:     $ROOT/runtime"
wait
