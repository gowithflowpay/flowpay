# FlowPay CLI

Install with Node.js 20 or newer:

```sh
npm install -g https://api.pixuno.xyz/downloads/flowpay-cli.tgz
flowpay register
flowpay login
flowpay request 20 usdc eth
```

Registration prompts for business name, contact name, email and EVM settlement address. Confirm the emailed OTP. Login uses email and OTP only. `eth` currently means Ethereum Sepolia testnet.

Requests print a wallet address and checkout URL, then monitor the backend until confirmed. Use `--no-wait` to return immediately or `--json` for scripts. Resume with `flowpay payment wait PAYMENT_ID`. Read state with `flowpay payment status PAYMENT_ID --json`.

Existing merchants can run `flowpay init` and approve a device in the dashboard. Credentials are saved locally; `FLOWPAY_CONFIG_DIR` isolates agent configurations. `flowpay logout` removes local credentials. Never put tokens in chat or logs.

Agents can create requests, exchange checkout URLs, and monitor receipts. Wallet transfers require the payer's wallet; the CLI does not broadcast them.

Documentation: https://pixuno.xyz/docs
Source: https://github.com/gowithflowpay/flowpay
