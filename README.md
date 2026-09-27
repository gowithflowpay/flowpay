# FlowPay

FlowPay is a crypto-only payment orchestration platform for hosted checkout, multi-chain payment verification, merchant webhooks, and constrained recovery of exceptional transfers.

## Repository

The production workspace is in [`flowpay/`](flowpay/README.md).

- `flowpay/backend` - Rust API, worker, event relay, PostgreSQL persistence, and message orchestration
- `flowpay/contracts` - Solidity checkout and recovery contracts
- `flowpay/apps/merchant` - merchant dashboard, payment history, API keys, and payment-link creation
- `flowpay/apps/checkout` - hosted crypto checkout and customer recovery experience
- `flowpay/apps/mcp` - credential-safe MCP installer for ChatGPT, Codex, Claude, and other AI agents
- `flowpay/sdk` - client integration code

## AI integration

The FlowPay MCP can inspect a Next.js site, create the server-side crypto payment route, add a checkout component, store the FlowPay credential directly in `.env.local`, and verify the integration. The API key stays inside the MCP process and is never returned to the model.

See [`flowpay/apps/mcp/README.md`](flowpay/apps/mcp/README.md) for setup.

## Development

Detailed architecture, environment setup, local services, test commands, and security boundaries are documented in [`flowpay/README.md`](flowpay/README.md).
