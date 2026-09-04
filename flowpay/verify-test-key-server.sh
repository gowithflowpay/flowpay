#!/usr/bin/env bash
set -euo pipefail
cd /root/flowpay/flow-pay/flowpay
set -a
source .env
set +a
created="$(curl -fsS -X POST http://127.0.0.1:8080/v1/api-keys -H 'content-type: application/json' -H "x-flowpay-api-key: ${FLOWPAY_API_KEY}" --data '{"name":"Environment verification key","environment":"test"}')"
id="$(jq -r '.id' <<<"$created")"
public="$(jq -r '.public_key' <<<"$created")"
secret="$(jq -r '.secret_key' <<<"$created")"
credential="$(jq -r '.api_key' <<<"$created")"
[[ "$public" == fp_test_* && "$credential" == "$public.$secret" ]]
curl -fsS http://127.0.0.1:8080/v1/api-keys -H "x-flowpay-api-key: ${credential}" | jq -e --arg id "$id" '.data[] | select(.id==$id and .revoked==false)' >/dev/null
curl -fsS -X POST "http://127.0.0.1:8080/v1/api-keys/${id}/revoke" -H "x-flowpay-api-key: ${credential}" >/dev/null
psql "$DATABASE_URL" -X -qAt -v ON_ERROR_STOP=1 -c "DELETE FROM api_keys WHERE label='Environment verification key' AND revoked_at IS NOT NULL;" >/dev/null
echo live_test_environment_generation=verified
