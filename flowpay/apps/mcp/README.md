# FlowPay MCP

Remote server: **https://mcp.pixuno.xyz/mcp**

Add this URL in a compatible ChatGPT or Claude remote MCP connector. FlowPay advertises protected-resource metadata and OAuth authorization-server metadata. The client opens the FlowPay consent page, where the user signs in by email, reviews payment permissions, and approves access.

OAuth uses authorization code with PKCE S256, exact registered redirect URIs, resource audience checks, short-lived access tokens and rotating refresh tokens. Approval and revocation are stored in PostgreSQL. Disconnect apps from Developers in the merchant dashboard. Tokens never appear in tool responses or prompts.

Hosted tools provide an integration guide, server-side Next.js checkout generation, credential verification, payment listing, payment lookup and payment creation. Payment tools enforce `payments:read` or `payments:write`. Creating a request does not broadcast a wallet transfer.

## Local stdio installer

Build `apps/mcp` with `npm ci` and `npm run build`. Configure the MCP client to launch `node /absolute/path/to/apps/mcp/dist/index.js`, with `FLOWPAY_PROJECT_ROOT` set to your site's directory. Run `flowpay login` or `flowpay init` first. The installer reads the local CLI configuration for the same API URL; an email session is exchanged for a separate revocable integration key. Use `FLOWPAY_CONFIG_DIR` to select an isolated agent configuration.

Explicit `FLOWPAY_API_KEY` or `FLOWPAY_MERCHANT_TOKEN` environment credentials are also supported for local automation. Store them in the client configuration or secret manager. The installer writes server-only `.env.local` with owner permissions and returns redacted status. Generated browser code never includes the merchant key.

Documentation: https://pixuno.xyz/docs#mcp
