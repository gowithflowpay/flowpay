#!/usr/bin/env bash
set -euo pipefail
cd /root/flowpay/flow-pay/flowpay
set -a
source .env
set +a
created="$(curl -fsS -X POST http://127.0.0.1:8080/v1/api-keys -H 'content-type: application/json' -H "x-flowpay-api-key: ${FLOWPAY_API_KEY}" --data '{"name":"Deployment verification key"}')"
echo stage=create_ok
key_id="$(jq -r '.id' <<<"$created")"
public_key="$(jq -r '.public_key' <<<"$created")"
secret_key="$(jq -r '.secret_key' <<<"$created")"
credential="$(jq -r '.api_key' <<<"$created")"
[[ "$public_key" == fp_live_* ]]
[[ -n "$secret_key" && "$secret_key" != "null" ]]
[[ "$credential" == "$public_key.$secret_key" ]]
echo stage=response_shape_ok
curl -fsS http://127.0.0.1:8080/v1/api-keys -H "x-flowpay-api-key: ${credential}" | jq -e --arg id "$key_id" '.data[] | select(.id==$id and .revoked==false and .created_at!=null)' >/dev/null
echo stage=authentication_and_list_ok
curl -fsS -X POST "http://127.0.0.1:8080/v1/api-keys/${key_id}/revoke" -H "x-flowpay-api-key: ${credential}" | jq -e '.revoked==true' >/dev/null
printf 'real_key_create_auth_list_revoke=verified\n'
all_keys="$(curl -fsS http://127.0.0.1:8080/v1/api-keys -H "x-flowpay-api-key: ${FLOWPAY_API_KEY}")"
while IFS= read -r cleanup_id; do
  [[ -z "$cleanup_id" ]] && continue
  curl -fsS -X POST "http://127.0.0.1:8080/v1/api-keys/${cleanup_id}/revoke" -H "x-flowpay-api-key: ${FLOWPAY_API_KEY}" >/dev/null
done < <(jq -r '.data[] | select(.name=="Deployment verification key" and .revoked==false) | .id' <<<"$all_keys")
echo verification_keys_cleanup=complete
