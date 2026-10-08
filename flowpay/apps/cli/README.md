# FlowPay CLI

Install with Node.js 20 or newer:

```sh
npm install -g https://api.pixuno.xyz/downloads/flowpay-cli.tgz
flowpay init
flowpay request 20 usdc eth
```

The terminal wizard asks for business name, recovery email and EVM settlement wallet. It generates an Ed25519 device key locally, proves possession with a signed server challenge, and signs you in immediately. No password, email OTP or browser approval is required. The recovery email is recorded without claiming it has been verified. `eth` currently means Ethereum Sepolia testnet.

Requests print a wallet address and checkout URL, then monitor the backend until confirmed. Use `--no-wait` to return immediately or `--json` for scripts. Resume with `flowpay payment wait PAYMENT_ID`. Read state with `flowpay payment status PAYMENT_ID --json`.

Interactive requests open the HTTPS checkout in your browser. Use `--no-open` to keep the browser closed or `--open` to explicitly open checkout. JSON automation never launches a browser.

Run `flowpay login` to sign in again automatically with the same device key. `flowpay logout` revokes the session and keeps the device key for later sign-in. Keys are scoped to the server origin and stored separately from config, encrypted with Windows DPAPI on Windows and restricted to the current user by file permissions on Unix; never share `.flowpay/*.key`. HTTP authentication is restricted to localhost. `FLOWPAY_CONFIG_DIR` isolates configurations. Existing browser accounts can optionally use `flowpay device` to approve a device. A new device cannot claim an existing account by entering its recovery email; account recovery and additional device enrollment require a separate ownership proof.

This source requires the backend device-key endpoints and migration `0019_cli_device_keys.sql`. The public download must be updated together with the backend before this flow is available live.

Agents can create requests, exchange checkout URLs, and monitor receipts. Wallet transfers require the payer's wallet; the CLI does not broadcast them.

Documentation: https://pixuno.xyz/docs
Source: https://github.com/gowithflowpay/flowpay
