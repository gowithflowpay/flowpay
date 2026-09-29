#!/bin/sh
set -eu
. /runtime/local.env
export PGPASSWORD=${POSTGRES_PASSWORD:-flowpay}
API_HASH=${FLOWPAY_DEMO_API_HASH:-a962c8a2c71378826d939c11b8d2c3c9c6d3ff64e4e36f430c1e03572d0154b1}
psql -h postgres -U flowpay -d flowpay -v ON_ERROR_STOP=1 \
  -v api_hash="$API_HASH" -v settlement="$FLOWPAY_LOCAL_SETTLEMENT" \
  -v base_usdc="$BASE_USDC_ADDRESS" -v base_usdt="$BASE_USDT_ADDRESS" \
  -v bsc_usdc="$BSC_USDC_ADDRESS" -v bsc_usdt="$BSC_USDT_ADDRESS" -v bsc_fail="$BSC_FAIL_ADDRESS" <<'SQL'
INSERT INTO merchants(id,public_id,name,status,evm_settlement_address) VALUES
('11111111-1111-4111-8111-111111111111','mer_demo','LumaBot','ACTIVE',:'settlement')
ON CONFLICT (id) DO UPDATE SET evm_settlement_address=EXCLUDED.evm_settlement_address;
INSERT INTO api_keys(merchant_id,label,public_prefix,secret_hash) VALUES
('11111111-1111-4111-8111-111111111111','Demo key','fp_test_demo',:'api_hash')
ON CONFLICT (merchant_id,public_prefix) DO UPDATE SET secret_hash=EXCLUDED.secret_hash,revoked_at=NULL;
DELETE FROM chain_assets WHERE chain IN ('base','bsc');
INSERT INTO chain_assets(chain,symbol,token_contract,decimals,purpose,enabled) VALUES
('base','USDC',:'base_usdc',6,'BOTH',true),
('base','USDT',:'base_usdt',6,'RECOVERY',true),
('bsc','USDC',:'bsc_usdc',6,'RECOVERY',true),
('bsc','USDT',:'bsc_usdt',6,'RECOVERY',true),
('bsc','FAIL',:'bsc_fail',6,'RECOVERY',true);
SQL
