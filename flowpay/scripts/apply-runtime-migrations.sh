#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
set -a
[[ -f .env ]] && source .env
set +a
DATABASE_URL="${DATABASE_URL:-postgres://flowpay:flowpay@127.0.0.1:5432/flowpay}"

cursor_key="$(psql "$DATABASE_URL" -Atc "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conname='payment_monitor_cursors_pkey' AND conrelid='payment_monitor_cursors'::regclass")"
if [[ "$cursor_key" != *"payment_id, chain"* ]]; then
  psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f backend/database/migrations/0009_chain_aware_monitoring.sql
fi
