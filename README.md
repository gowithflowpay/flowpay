# FlowPay

FlowPay is a crypto-only payment orchestration platform for hosted checkout, multi-chain payment verification, merchant webhooks, and constrained recovery of exceptional transfers.

## Repository

The production workspace is in [`flowpay/`](flowpay/README.md).

- `flowpay/backend` - Rust API, worker, event relay, PostgreSQL persistence, and message orchestration
- `flowpay/contracts` - Solidity checkout and recovery contracts
- `flowpay/apps/merchant` - merchant dashboard, payment history, API keys, and payment-link creation
- `flowpay/apps/checkout` - hosted crypto checkout and customer recovery experience
- `flowpay/apps/mcp` - credential-safe local and hosted MCP for ChatGPT, Codex, Claude, and other AI agents
- `flowpay/apps/cli` - terminal payments, email sign-in and dashboard-approved device access
- `flowpay/sdk` - client integration code

## AI integration

The FlowPay MCP can generate a server-side Next.js crypto payment route and checkout component, verify credentials, create hosted crypto payments, and read payment history. The hosted service uses Streamable HTTP and OAuth authorization with PKCE; users approve payment permissions on FlowPay's consent page. The local installer can reuse CLI credentials and write the integration into a project. Credentials remain in local or deployment secret storage and never appear in tool responses.

See [`flowpay/apps/mcp/README.md`](flowpay/apps/mcp/README.md) for setup.

Email sign-in and device setup are documented in [`flowpay/docs/cli.md`](flowpay/docs/cli.md).
Local verification output, old deployment bundles and configuration backups live in
the ignored `.local/` directory. Browser profiles and build caches are ignored.

## Quick start

Requires Node.js 20 or newer.

```sh
npm install -g https://api.pixuno.xyz/downloads/flowpay-cli.tgz
flowpay register
flowpay login
flowpay request 20 usdc eth
```

The `eth` alias currently uses Ethereum Sepolia testnet. Requests print a wallet address and checkout URL, then wait for confirmation. Agents can use `--json` for structured output. Wallet transfers are submitted by the payer's wallet.

Public documentation: [pixuno.xyz/docs](https://pixuno.xyz/docs).
Remote MCP: [mcp.pixuno.xyz/mcp](https://mcp.pixuno.xyz/mcp).

## Development

Detailed architecture, environment setup, local services, test commands, and security boundaries are documented in [`flowpay/README.md`](flowpay/README.md).
