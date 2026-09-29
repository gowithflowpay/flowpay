# CREATE3 / Counterfactual Checkout Design

## Chosen mechanism

FlowPay will use the established two-step CREATE3 pattern used by implementations such as
Solmate/Solady/0xSequence:

1. `FlowPayFactory` deploys a fixed tiny proxy with `CREATE2(factory, salt, proxy_initcode)`.
2. The factory calls that proxy with the checkout receiver creation code.
3. The proxy uses `CREATE` for its first child deployment (nonce = 1).
4. Therefore the final receiver address depends on the proxy address, not on receiver initcode.

The final address is therefore predictable before the receiver exists.

## Exact derivation

Let:

```text
P = keccak256(0xff ++ factory ++ salt ++ keccak256(PROXY_INITCODE))[12:]
R = keccak256(rlp([P, 1]))[12:]
```

For a 20-byte proxy address and nonce 1, the RLP bytes are:

```text
0xd6 0x94 <20-byte proxy> 0x01
```

Thus:

```text
R = keccak256(0xd694 ++ P ++ 0x01)[12:]
```

`R` is the counterfactual checkout receiver address.

## Critical consequence

CREATE3 does **not** make addresses magically chain-independent. The final address is equal on
Base and BSC only when all address inputs are equal:

- same factory address;
- same salt;
- same proxy initcode hash;
- compatible EVM CREATE2/CREATE semantics.

FlowPay therefore treats factory deployment as protocol infrastructure, not an incidental
contract deployment.

## Factory design requirements

The factory must not expose a public arbitrary-initcode deployment primitive for FlowPay checkout
salts. Otherwise an attacker could front-run the first deployment and install malicious child
code at a prefunded checkout address.

Milestone 2 factory requirements:

- fixed CREATE3 proxy implementation/hash;
- only an allowlisted deployer/recovery executor can instantiate a checkout receiver;
- checkout receiver creation code is fixed by the factory or checked against an approved hash;
- duplicate deployment reverts cleanly;
- factory exposes pure/view prediction;
- factory emits deployment events;
- factory ownership/role changes are auditable and preferably timelocked/multisig in production;
- no upgrade path may silently change the prediction formula for an existing address domain.

## Checkout receiver requirements

Keep the receiver minimal. It has no AI and no arbitrary call primitive.

Proposed operations:

- `sweepERC20(token, destination, amount)` restricted to factory/recovery executor;
- `sweepNative(destination, amount)` restricted likewise;
- destination must be supplied by the policy-bound recovery transaction, not chosen by the agent;
- optional one-time initialization immutables/constructor values are acceptable because CREATE3
  final address does not depend on child initcode, but the factory still fixes/checks the code.

The contract should avoid `SELFDESTRUCT`; modern EVM semantics make redeployment-based patterns
unsafe and unnecessary.

## Prefunding behavior to prove in Milestone 2

### ERC-20
An ERC-20 transfer records balance in the token contract keyed by the destination address.
The destination does not need code at transfer time. After the receiver is deployed at that same
address, its code can transfer the balance subject to token behavior.

### Native asset
An address may hold native balance before contract creation. Balance alone is not the same as
existing code/nonzero nonce for CREATE2 collision purposes. Milestone 2 must prove the selected
local EVM behavior with a test that prefunds the receiver address, deploys the receiver, and sweeps.

## Salt design

```text
DOMAIN = keccak256("FLOWPAY_EVM_CHECKOUT_V1")
salt   = keccak256(abi.encode(DOMAIN, merchant_uuid, payment_uuid))
```

Do not use `abi.encodePacked` with variable user-controlled strings. Do not use merchant
`reference` values. Both UUID inputs are immutable binary identifiers.

### Why chain ID is not in the salt

Including chain ID would intentionally produce different addresses and break the flagship
wrong-EVM-chain recovery case. Instead:

- payment.expected_chain is persisted and enforced independently;
- the same EVM address family can exist on Base/BSC;
- an accidental transfer on another chain is never interpreted as a valid payment;
- recovery on another chain requires factory verification + policy + claim authorization.

Solana uses a separate domain and address model later.

## Replay considerations

- A Base payment is never satisfied by a BSC deposit even when the address is identical.
- Chain ID is included in signed claim challenges and approval/execution records.
- RecoveryPlan includes source chain and exact token contract.
- Approval binds a hash of the complete plan, not only claim ID.
- Signer rejects chain mismatch, token mismatch, destination mismatch, expired/replayed approval,
  and unapproved transaction class.

## Public-testnet deployment strategy

For a same-address Base/BSC demo, FlowPay must deploy the factory to the same address on both
networks. This can be achieved only with a deterministic bootstrap strategy whose prerequisites
are verified on both networks. Until that bootstrap is proven, local dual-Anvil chains are the
canonical reproducible cross-chain evaluation environment.

Public testnet support must not be advertised as cross-chain recoverable merely because both
networks are EVM compatible.

## Tests required before declaring CREATE3 complete

1. pure address prediction matches contract prediction;
2. same factory + salt -> same predicted address on two local EVM chains;
3. different factory -> different final address;
4. different salt -> different final address;
5. prefund ERC-20 before receiver deployment -> deploy -> sweep succeeds;
6. prefund native asset before deployment -> deploy -> sweep succeeds;
7. unauthorized deploy attempt fails;
8. wrong receiver initcode/hash fails;
9. duplicate deployment fails without corrupting funds;
10. wrong factory/code-hash check yields NOT_RECOVERABLE/ESCALATE;
11. malicious recovery destination rejected by policy/signer;
12. unsupported ERC-20 behavior escalates instead of assuming transfer success.

## References used for design validation

- EIP-1014 (CREATE2): https://eips.ethereum.org/EIPS/eip-1014
- EIP-6780 (modern SELFDESTRUCT semantics): https://eips.ethereum.org/EIPS/eip-6780
- Solmate CREATE3: https://github.com/transmissions11/solmate/blob/main/src/utils/CREATE3.sol
- Solady CREATE3: https://github.com/Vectorized/solady/blob/main/src/utils/CREATE3.sol
- 0xSequence CREATE3: https://github.com/0xSequence/create3

## Independent Milestone 0 test vector

The following vector was independently recomputed with an external Keccak-256 implementation
(OpenSSL's `KECCAK-256` digest), not by a CREATE3 Solidity library:

```text
factory       = 0x1111111111111111111111111111111111111111
salt          = 0x0000000000000000000000000000000000000000000000000000000000000000
proxy_initcode= 0x67363d3d37363d34f03d5260086018f3
proxy_hash    = 0x21c35dbe1b344a2488cf3321d6ce542f8e9f305544ff09e4993a62319a497c1f
proxy_address = 0x488b2cae86f54262a91d6fa79d9cf0239dcf2a24
receiver      = 0x177417469513f82ba9d65ba78ea3d791b305cb57
```

Milestone 2 contract tests must reproduce this vector exactly before FlowPay treats the
prediction implementation as correct.
