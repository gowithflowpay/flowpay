#!/usr/bin/env bash
set -euo pipefail
[[ -f runtime/local.env ]] || { echo "run scripts/deploy-contracts.sh first" >&2; exit 1; }
set -a; source runtime/local.env; source .env 2>/dev/null || true; set +a
PEPPER="${FLOWPAY_API_KEY_HASH_PEPPER:-local-dev-pepper-change-me}"
API_KEY="${FLOWPAY_DEMO_API_KEY:-fp_test_demo.7d7c509e6b55469f9a3c66f87d7ebc52}"
API_PREFIX="${API_KEY%%.*}"
HASH=$(node -e "const c=require('crypto');process.stdout.write(c.createHash('sha256').update(process.argv[1]+process.argv[2]).digest('hex'))" "$PEPPER" "$API_KEY")
DATABASE_URL="${DATABASE_URL:-postgres://flowpay:flowpay@127.0.0.1:5432/flowpay}"
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 \
  -v api_prefix="$API_PREFIX" -v api_hash="$HASH" -v settlement="$FLOWPAY_LOCAL_SETTLEMENT" \
  -v base_usdc="$BASE_USDC_ADDRESS" -v base_usdt="$BASE_USDT_ADDRESS" \
  -v bsc_usdc="$BSC_USDC_ADDRESS" -v bsc_usdt="$BSC_USDT_ADDRESS" -v bsc_fail="$BSC_FAIL_ADDRESS" <<'SQL'
INSERT INTO merchants(id,public_id,name,status,evm_settlement_address) VALUES
('11111111-1111-4111-8111-111111111111','mer_demo','LumaBot','ACTIVE',:'settlement')
ON CONFLICT (id) DO UPDATE SET name=EXCLUDED.name,evm_settlement_address=EXCLUDED.evm_settlement_address;
INSERT INTO api_keys(merchant_id,label,public_prefix,secret_hash) VALUES
('11111111-1111-4111-8111-111111111111','Demo key',:'api_prefix',:'api_hash')
ON CONFLICT (merchant_id,public_prefix) DO UPDATE SET secret_hash=EXCLUDED.secret_hash,revoked_at=NULL;
DELETE FROM chain_assets WHERE chain IN ('base','bsc');
INSERT INTO chain_assets(chain,symbol,token_contract,decimals,purpose,enabled) VALUES
('base','USDC',:'base_usdc',6,'BOTH',true),
('base','USDT',:'base_usdt',6,'RECOVERY',true),
('bsc','USDC',:'bsc_usdc',6,'RECOVERY',true),
('bsc','USDT',:'bsc_usdt',6,'RECOVERY',true),
('bsc','FAIL',:'bsc_fail',6,'RECOVERY',true);
SQL
echo "FLOWPAY_DEMO_API_KEY=$API_KEY"
