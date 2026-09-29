#!/usr/bin/env bash
set -euo pipefail
command -v psql >/dev/null || { echo "psql is required" >&2; exit 1; }
set -a; [[ -f .env ]] && source .env; set +a
DATABASE_URL="${DATABASE_URL:-postgres://flowpay:flowpay@127.0.0.1:5432/flowpay}"
for f in backend/database/migrations/*.sql; do
  echo "Applying $f"
  psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f "$f"
done
