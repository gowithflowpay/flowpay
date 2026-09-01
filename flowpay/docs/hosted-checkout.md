# Hosted checkout

The customer-facing FlowPay app lives in `apps/checkout` and runs independently from the merchant dashboard.

## Payment URL

`POST /v1/payments` returns a `checkout_url` such as:

```text
https://pixuno.xyz/pay/pay_<id>
```

The checkout route never fabricates payment details. It loads the payment through the FlowPay API and renders:

- merchant display name
- expected token amount
- stablecoin USD display value for USDC/USDT
- expected chain
- deterministic checkout address
- offline-generated QR code
- payment expiry countdown
- live payment state
- detected partial-deposit count when available
- recovery-claim link

The browser polls the deterministic payment API every 3.5 seconds until the payment enters a terminal state. Provider notifications are not trusted by the UI; critical state transitions still happen in the backend after chain verification.

## Claim experience

The recovery UI follows four explicit stages:

1. Claim details - what happened, optional evidence, recovery destination.
2. Transaction information - full transaction hash, actual chain, actual token, actual amount, optional date/time.
3. Verify ownership - connect an injected EVM wallet. No private key or seed phrase is requested or stored.
4. Review and submit - the customer reviews the claim before investigation begins.

On submission, FlowPay creates the claim and returns a one-time wallet challenge. The browser asks the connected wallet to sign that exact challenge with `personal_sign`, then sends the signature to the authorization endpoint. The backend independently verifies the EIP-191 signature.

WalletConnect is intentionally marked unavailable in this build until the WalletConnect SDK is actually integrated. This avoids presenting a non-functional wallet path as complete.

## Assets

All product artwork is stored under `apps/checkout/public/assets` as SVG, including:

- FlowPay mark
- recovery agent illustration
- Base and BNB Smart Chain marks
- USDC and USDT marks
- merchant storefront illustration
- wallet-provider marks

SVG was chosen so checkout visuals remain crisp at mobile, Retina and 4K resolutions without shipping large raster files.

## Environment

The hosted checkout server needs:

```text
FLOWPAY_API_URL=http://127.0.0.1:8080
FLOWPAY_CHECKOUT_API_KEY=<server-side FlowPay API key>
```

`FLOWPAY_CHECKOUT_API_KEY` is server-only. It must never be exposed through a `NEXT_PUBLIC_` variable.
