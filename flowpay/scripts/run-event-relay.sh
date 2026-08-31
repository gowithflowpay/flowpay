#!/usr/bin/env bash
set -euo pipefail
[[ -f /root/.cargo/env ]] && source /root/.cargo/env
set -a; [[ -f .env ]] && source .env; set +a
export DATABASE_URL="${DATABASE_URL:-postgres://flowpay:flowpay@127.0.0.1:5432/flowpay}"
export RABBITMQ_URL="${RABBITMQ_URL:-amqp://guest:guest@127.0.0.1:5672/%2f}"
export KAFKA_BROKERS="${KAFKA_BROKERS:-127.0.0.1:9092}"
BIN=backend/target/debug/flowpay-event-relay
if [[ -x "$BIN" ]]; then exec "$BIN"; fi
exec cargo run --manifest-path backend/Cargo.toml -q -p flowpay-event-relay
