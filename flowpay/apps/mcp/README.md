# FlowPay MCP

FlowPay MCP supports both a local installer and a hosted Streamable HTTP service. It installs a server-side, crypto-only FlowPay checkout without returning the FlowPay API key to the model.

## Hosted MCP

Connect an MCP-compatible client to `https://mcp.pixuno.xyz/mcp` and store a live FlowPay API key as the connector's Bearer token. The credential is validated against FlowPay before MCP initialization and is never included in tool output.

Available hosted tools:

- `flowpay_integration_guide`
- `flowpay_generate_nextjs_integration`
- `flowpay_verify_credentials`
- `flowpay_list_payments`
- `flowpay_get_payment`
- `flowpay_create_payment`

The generated browser component calls a server-only route. Only that route reads `FLOWPAY_API_KEY` from the deployment secret manager.

## Local installer

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

## ChatGPT

Run this private stdio server beside the site repository and connect it to ChatGPT through Secure MCP Tunnel. The tunnel keeps the project files and FlowPay credential on the developer machine while ChatGPT discovers and calls the MCP tools. After connecting it in ChatGPT developer mode, a prompt such as `Integrate FlowPay crypto checkout into this site and verify it` runs the inspect, install, and verification workflow without placing the API key in model context.
