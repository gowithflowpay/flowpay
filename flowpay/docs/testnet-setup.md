# Testnet setup

FlowPay supports these EVM testnet keys when configured:

- `ethereum_sepolia` — chain ID 11155111
- `base_sepolia` — chain ID 84532
- `arbitrum_sepolia` — chain ID 421614
- `optimism_sepolia` — chain ID 11155420
- `polygon_amoy` — chain ID 80002
- `bsc_testnet` — chain ID 97

## Required configuration

A testnet is enabled only when its RPC URL and factory address are configured. Do not reuse a
factory address from another network unless the deployment has been independently verified.

```env
FLOWPAY_ENV=testnet
DATABASE_URL=postgres://...
FLOWPAY_FACTORY_ADDRESS=0x0000000000000000000000000000000000000000
FLOWPAY_PROXY_CREATION_CODE_HASH=0x...
FLOWPAY_OPERATOR_ADDRESS=0x...
FLOWPAY_API_KEY_HASH_PEPPER=...
FLOWPAY_WEBHOOK_ENCRYPTION_KEY=...

BASE_SEPOLIA_RPC_URL=https://...
BASE_SEPOLIA_FACTORY_ADDRESS=0x...
BASE_SEPOLIA_CHAIN_ID=84532

ETHEREUM_SEPOLIA_RPC_URL=https://...
ETHEREUM_SEPOLIA_FACTORY_ADDRESS=0x...
ETHEREUM_SEPOLIA_CHAIN_ID=11155111

ARBITRUM_SEPOLIA_RPC_URL=https://...
ARBITRUM_SEPOLIA_FACTORY_ADDRESS=0x...
ARBITRUM_SEPOLIA_CHAIN_ID=421614

OPTIMISM_SEPOLIA_RPC_URL=https://...
OPTIMISM_SEPOLIA_FACTORY_ADDRESS=0x...
OPTIMISM_SEPOLIA_CHAIN_ID=11155420

POLYGON_AMOY_RPC_URL=https://...
POLYGON_AMOY_FACTORY_ADDRESS=0x...
POLYGON_AMOY_CHAIN_ID=80002

BSC_TESTNET_RPC_URL=https://...
BSC_TESTNET_FACTORY_ADDRESS=0x...
BSC_TESTNET_CHAIN_ID=97
```

The global factory variables are retained for compatibility with the existing local deployment
scripts. Public testnet deployments should use the per-network factory variables.

## Alchemy notifications

Configure Alchemy Notify to POST to:

```text
https://<your-api-host>/v1/providers/alchemy/webhook
```

Set the webhook signing secret as `FLOWPAY_PROVIDER_WEBHOOK_SECRET`. Provider notifications are
only low-latency triggers. FlowPay still verifies the transaction, receipt, token, recipient,
amount, canonical block, and confirmation depth through the configured RPC before changing payment
state.

## Asset registry

Every token must be explicitly inserted into `chain_assets` with the correct chain key, contract,
decimals, and purpose. Never accept a user-supplied contract as an implicitly supported asset.

## Recovery

Recovery is disabled until factory verification passes on the source chain. Cross-chain recovery
requires independently verified compatible factory deployments and matching CREATE3 prediction
vectors. A configured RPC URL by itself does not enable recovery.
