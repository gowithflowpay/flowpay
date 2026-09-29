#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${FLOWPAY_API_KEY:?FLOWPAY_API_KEY is required}"
: "${FLOWPAY_API_KEY_HASH_PEPPER:?FLOWPAY_API_KEY_HASH_PEPPER is required}"
: "${FLOWPAY_TESTNET_SETTLEMENT_ADDRESS:?FLOWPAY_TESTNET_SETTLEMENT_ADDRESS is required}"
: "${BASE_SEPOLIA_USDC_ADDRESS:?BASE_SEPOLIA_USDC_ADDRESS is required}"
: "${ETHEREUM_SEPOLIA_USDC_ADDRESS:?ETHEREUM_SEPOLIA_USDC_ADDRESS is required}"
: "${ARBITRUM_SEPOLIA_USDC_ADDRESS:?ARBITRUM_SEPOLIA_USDC_ADDRESS is required}"
: "${BSC_TESTNET_USDC_ADDRESS:?BSC_TESTNET_USDC_ADDRESS is required}"

API_PREFIX="${FLOWPAY_API_KEY%%.*}"
API_HASH="$(node -e "const c=require('crypto');process.stdout.write(c.createHash('sha256').update(process.argv[1]+process.argv[2]).digest('hex'))" "$FLOWPAY_API_KEY_HASH_PEPPER" "$FLOWPAY_API_KEY")"

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 \
  -v api_prefix="$API_PREFIX" \
  -v api_hash="$API_HASH" \
  -v settlement="$FLOWPAY_TESTNET_SETTLEMENT_ADDRESS" \
  -v base_usdc="$BASE_SEPOLIA_USDC_ADDRESS" \
  -v eth_usdc="$ETHEREUM_SEPOLIA_USDC_ADDRESS" \
  -v arb_usdc="$ARBITRUM_SEPOLIA_USDC_ADDRESS" \
  -v bsc_usdc="$BSC_TESTNET_USDC_ADDRESS" <<'SQL'
INSERT INTO merchants(id,public_id,name,status,evm_settlement_address)
VALUES ('11111111-1111-4111-8111-111111111111','mer_testnet','FlowPay Testnet','ACTIVE',:'settlement')
ON CONFLICT (id) DO UPDATE SET evm_settlement_address=EXCLUDED.evm_settlement_address;

INSERT INTO api_keys(merchant_id,label,public_prefix,secret_hash) VALUES
('11111111-1111-4111-8111-111111111111','Testnet dashboard',:'api_prefix',:'api_hash')
ON CONFLICT (merchant_id,public_prefix)
DO UPDATE SET secret_hash=EXCLUDED.secret_hash,revoked_at=NULL;

DELETE FROM chain_assets
WHERE chain IN ('base_sepolia','ethereum_sepolia','arbitrum_sepolia','bsc_testnet')
  AND upper(symbol) IN ('USDC','ETH');

INSERT INTO chain_assets(chain,symbol,token_contract,decimals,purpose,enabled) VALUES
('base_sepolia','USDC',:'base_usdc',6,'BOTH',true),
('base_sepolia','ETH',NULL,18,'BOTH',true),
('ethereum_sepolia','USDC',:'eth_usdc',6,'BOTH',true),
('ethereum_sepolia','ETH',NULL,18,'BOTH',true),
('arbitrum_sepolia','USDC',:'arb_usdc',6,'BOTH',true),
('bsc_testnet','USDC',:'bsc_usdc',6,'BOTH',true)
ON CONFLICT DO NOTHING;
SQL
printf '%s\n' 'Testnet USDC and native ETH assets seeded.'
