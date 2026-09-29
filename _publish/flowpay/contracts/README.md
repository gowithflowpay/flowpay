# FlowPay EVM contracts

`FlowPayFactory` deploys a constant `CheckoutProxy` with CREATE2. The proxy captures the
factory as controller and allows exactly one CREATE. The first CREATE address is therefore
counterfactually computable before the receiver exists. `FlowPayFactory` supplies a fixed
`CheckoutReceiver` initcode; callers cannot choose arbitrary child bytecode.

The final address is:

```text
proxy = keccak256(0xff ++ factory ++ salt ++ keccak256(CheckoutProxy.creationCode))[12:]
receiver = keccak256(0xd694 ++ proxy ++ 0x01)[12:]
```

Cross-chain equality requires identical factory address, salt and proxy creation code.
The local dual-Anvil scripts intentionally deploy the factory first from the same deterministic
account/nonce on both chains. Public testnet deployments must verify this invariant rather than
assume it.

Run:

```bash
forge test -vvv
```
