#!/usr/bin/env bash
set -euo pipefail
cd /root/flowpay/flow-pay/flowpay
set -a
source .env
set +a
deleted="$(psql "$DATABASE_URL" -X -qAt -v ON_ERROR_STOP=1 -c "WITH removed AS (DELETE FROM api_keys WHERE label='Deployment verification key' AND revoked_at IS NOT NULL RETURNING 1) SELECT count(*) FROM removed;")"
printf 'verification_rows_removed=%s\n' "$deleted"
