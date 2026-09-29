#!/usr/bin/env bash
set -euo pipefail
for tool in psql pg_isready anvil forge cast jq node; do command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }; done
mkdir -p runtime
cp -n .env.example .env 2>/dev/null || true
set -a; source .env; set +a
DATABASE_URL="${DATABASE_URL:-postgres://flowpay:flowpay@127.0.0.1:5432/flowpay}"
pg_isready -d "$DATABASE_URL" >/dev/null || { echo "PostgreSQL is not ready at $DATABASE_URL" >&2; exit 1; }
./scripts/setup-local-chain.sh
./scripts/deploy-contracts.sh
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL' >/dev/null
DROP SCHEMA public CASCADE;
CREATE SCHEMA public;
SQL
./scripts/migrate.sh
./scripts/seed.sh
rm -rf runtime/evidence
mkdir -p runtime/evidence
echo "FlowPay state reset: PostgreSQL + fresh Base/BSC Anvil chains. RabbitMQ and Kafka must already be running."
