# FlowPay MCP

This MCP installs a server-side FlowPay crypto checkout into a Next.js site. The model can inspect, install, and verify the integration without receiving the FlowPay API key.

```json
{
  "mcpServers": {
    "flowpay": {
      "command": "node",
      "args": ["/path/to/flowpay/apps/mcp/dist/index.js"],
      "env": {
        "FLOWPAY_PROJECT_ROOT": "/path/to/your/site",
        "FLOWPAY_API_URL": "https://api.pixuno.xyz",
        "FLOWPAY_MERCHANT_TOKEN": "merchant-session-token"
      }
    }
  }
}
```

`FLOWPAY_API_KEY` can be supplied instead of a merchant token. Credentials belong in the MCP client configuration or a secret manager, never in chat. The installer writes `.env.local` with owner-only permissions and only returns redacted status.
