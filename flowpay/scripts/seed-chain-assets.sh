#!/usr/bin/env bash
# Seeds the global chain/token asset list that payments and the dashboard read.
# This is platform configuration, not merchant data, so it stands on its own and
# is safe to re-run: every insert is guarded and the native-asset rows use a
# NULL contract, which the unique index treats as a single row per chain.
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 \
  -v base_usdc="${BASE_SEPOLIA_USDC_ADDRESS:-}" \
  -v eth_usdc="${ETHEREUM_SEPOLIA_USDC_ADDRESS:-}" \
  -v arb_usdc="${ARBITRUM_SEPOLIA_USDC_ADDRESS:-}" \
  -v bsc_usdc="${BSC_TESTNET_USDC_ADDRESS:-}" \
  -v op_usdc="${OPTIMISM_SEPOLIA_USDC_ADDRESS:-}" \
  -v poly_usdc="${POLYGON_AMOY_USDC_ADDRESS:-}" <<'SQL'
-- Native gas assets need no contract address and are always available.
INSERT INTO chain_assets(chain,symbol,token_contract,decimals,purpose,enabled)
VALUES
  ('base_sepolia','ETH',NULL,18,'BOTH',true),
  ('ethereum_sepolia','ETH',NULL,18,'BOTH',true),
  ('arbitrum_sepolia','ETH',NULL,18,'BOTH',true),
  ('optimism_sepolia','ETH',NULL,18,'BOTH',true),
  ('bsc_testnet','BNB',NULL,18,'BOTH',true),
  ('polygon_amoy','POL',NULL,18,'BOTH',true)
ON CONFLICT (chain, lower(coalesce(token_contract, 'native'))) DO NOTHING;

-- Stablecoins are only seeded when the deployment knows their address.
INSERT INTO chain_assets(chain,symbol,token_contract,decimals,purpose,enabled)
SELECT chain, symbol, contract, 6, 'BOTH', true
FROM (VALUES
  ('base_sepolia',      'USDC', nullif(:'base_usdc', '')),
  ('ethereum_sepolia',  'USDC', nullif(:'eth_usdc', '')),
  ('arbitrum_sepolia',  'USDC', nullif(:'arb_usdc', '')),
  ('bsc_testnet',       'USDC', nullif(:'bsc_usdc', '')),
  ('optimism_sepolia',  'USDC', nullif(:'op_usdc', '')),
  ('polygon_amoy',      'USDC', nullif(:'poly_usdc', ''))
) AS seed(chain, symbol, contract)
WHERE contract IS NOT NULL
ON CONFLICT (chain, lower(coalesce(token_contract, 'native'))) DO NOTHING;
SQL

printf '%s\n' 'Chain assets seeded.'
psql "$DATABASE_URL" -Atc "SELECT chain||' '||symbol||' '||coalesce(token_contract,'native') FROM chain_assets WHERE enabled ORDER BY chain,symbol"
