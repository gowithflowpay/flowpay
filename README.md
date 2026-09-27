# FlowPay

FlowPay is a crypto-only payment orchestration platform for hosted checkout, multi-chain payment verification, merchant webhooks, and constrained recovery of exceptional transfers.

## Repository

The production workspace is in [`flowpay/`](flowpay/README.md).

- `flowpay/backend` - Rust API, worker, event relay, PostgreSQL persistence, and message orchestration
- `flowpay/contracts` - Solidity checkout and recovery contracts
- `flowpay/apps/merchant` - merchant dashboard, payment history, API keys, and payment-link creation
- `flowpay/apps/checkout` - hosted crypto checkout and customer recovery experience
- `flowpay/apps/mcp` - credential-safe local and hosted MCP for ChatGPT, Codex, Claude, and other AI agents
- `flowpay/sdk` - client integration code

## AI integration

The FlowPay MCP can generate a server-side Next.js crypto payment route and checkout component, verify credentials, create hosted crypto payments, and read payment history. The hosted service uses Streamable HTTP; the local installer can write the integration directly into a project. API keys stay in connector or deployment secret storage and are never returned to the model.

See [`flowpay/apps/mcp/README.md`](flowpay/apps/mcp/README.md) for setup.

## Development

Detailed architecture, environment setup, local services, test commands, and security boundaries are documented in [`flowpay/README.md`](flowpay/README.md).
