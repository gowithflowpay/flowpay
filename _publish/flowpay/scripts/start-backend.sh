#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
mkdir -p runtime

set -a
[[ -f .env ]] && source .env
set +a

MODE="${1:-${FLOWPAY_AGENT_MODE:-model}}"
case "$MODE" in
  model|deterministic|baseline) ;;
  *) echo "mode must be model, deterministic, or baseline" >&2; exit 2 ;;
esac

for tool in cargo curl nc pg_isready; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done

DATABASE_URL="${DATABASE_URL:-postgres://flowpay:flowpay@127.0.0.1:5432/flowpay}"
RABBITMQ_HOST="${FLOWPAY_RABBITMQ_HOST:-127.0.0.1}"
RABBITMQ_PORT="${FLOWPAY_RABBITMQ_PORT:-5672}"
KAFKA_HOST="${FLOWPAY_KAFKA_HOST:-127.0.0.1}"
KAFKA_PORT="${FLOWPAY_KAFKA_PORT:-9092}"

pg_isready -d "$DATABASE_URL" >/dev/null || {
  echo "PostgreSQL is not ready; start the native service and retry" >&2
  exit 1
}
./scripts/apply-runtime-migrations.sh
nc -z "$RABBITMQ_HOST" "$RABBITMQ_PORT" || {
  echo "RabbitMQ is not ready at $RABBITMQ_HOST:$RABBITMQ_PORT" >&2
  exit 1
}
nc -z "$KAFKA_HOST" "$KAFKA_PORT" || {
  echo "Kafka is not ready at $KAFKA_HOST:$KAFKA_PORT" >&2
  exit 1
}

if [[ "${FLOWPAY_ENV:-local}" == "local" && ! -f runtime/local.env ]]; then
  ./scripts/bootstrap-local.sh
fi

cargo build --manifest-path backend/Cargo.toml \
  -p flowpay-api-server -p flowpay-worker -p flowpay-event-relay

pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

./scripts/run-event-relay.sh >runtime/event-relay.log 2>&1 & pids+=("$!")
./scripts/run-worker.sh "$MODE" >runtime/worker.log 2>&1 & pids+=("$!")
./scripts/run-api.sh "$MODE" >runtime/api.log 2>&1 & pids+=("$!")

for _ in {1..60}; do
  if ! kill -0 "${pids[0]}" 2>/dev/null || ! kill -0 "${pids[1]}" 2>/dev/null || ! kill -0 "${pids[2]}" 2>/dev/null; then
    echo "A backend process exited during startup; inspect runtime/*.log" >&2
    exit 1
  fi
  curl -fsS http://127.0.0.1:8080/health >/dev/null 2>&1 && break
  sleep 1
done
curl -fsS http://127.0.0.1:8080/health >/dev/null || {
  echo "API did not become healthy; inspect runtime/api.log" >&2
  exit 1
}
sleep 1
for pid in "${pids[@]}"; do
  kill -0 "$pid" 2>/dev/null || {
    echo "A backend process exited during startup; inspect runtime/*.log" >&2
    exit 1
  }
done

echo "FlowPay backend is ready"
echo "API:  http://127.0.0.1:8080"
echo "Logs: $ROOT/runtime"
echo "Press Ctrl-C to stop all backend processes"
wait
