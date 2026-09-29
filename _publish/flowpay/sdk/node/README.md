# @flowpay/node

```ts
import { FlowPay } from "@flowpay/node";
const flowpay = new FlowPay({ apiKey: process.env.FLOWPAY_API_KEY! });
const payment = await flowpay.payments.create({amount:"10",asset:"USDC",chain:"base",reference:"ORDER_123"});
console.log(payment.address);
```

`verifyWebhookSignature` verifies the `t=...,v1=...` HMAC header with a five-minute replay window by default.
