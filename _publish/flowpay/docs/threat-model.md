# FlowPay Threat Model

## Security objective

FlowPay may classify uncertain cases with an agent, but no uncertain statement is allowed to
become a financial action without deterministic verification, policy evaluation, simulation,
approval where required, restricted signing, and receipt verification.

## Assets to protect

- merchant funds and recovered customer funds;
- signer private keys / key handles;
- merchant API keys and webhook secrets;
- checkout ownership mapping;
- payment and claim state integrity;
- recovery destination integrity;
- approvals and plan hashes;
- audit/trajectory integrity;
- uploaded evidence privacy;
- chain configuration and factory allowlist.

## Main adversaries

- claimant who knows a transaction hash but does not own the source wallet;
- merchant trying to recover funds not belonging to them;
- external attacker submitting forged evidence or replayed API requests;
- attacker front-running counterfactual receiver deployment;
- malicious/compromised RPC or webhook provider;
- malicious token contract;
- prompt injection inside textual claim evidence;
- compromised agent process;
- compromised API process;
- insider with partial service access.

## Threats and mitigations

| Threat | Impact | Mitigation | Safe failure |
|---|---|---|---|
| Fake tx hash | false payment/recovery | fetch tx + receipt from configured chain, verify recipient/log/token/block | escalate/reject |
| Wrong claimed chain | false recovery | query claimed chain only as a lead; verify chain ID and factory | not recoverable/escalate |
| Forged screenshot | false ownership | screenshots never authoritative | request cryptographic evidence/escalate |
| Tx-hash knowledge treated as ownership | theft | wallet challenge/signature for self-custody automatic recovery | escalate custodial ambiguity |
| Prompt injection in evidence | agent policy bypass | evidence is data; tools enforce authorization/policy independently | escalate |
| RPC lies/omits data | false state | configurable redundant verification/retry for critical facts; canonical block/hash checks | retry/escalate |
| Duplicate provider webhook | double processing | unique `(chain, tx_hash, log_index)` + idempotent handlers | no-op |
| Chain reorg | payment accepted too early | confirmation/finality policy + block-hash tracking + reversible pre-final states | hold/re-evaluate |
| Public factory arbitrary deploy | malicious receiver at prefunded address | factory access control + fixed/approved receiver code | deploy denied |
| Wrong factory on recovery chain | funds unrecoverable / takeover risk | code-hash + owner/role + prediction vector check | not recoverable |
| Malicious recovery destination | theft | destination bound to verified claimant and immutable RecoveryPlan; policy/signer recheck | reject |
| Agent arbitrary calldata | theft | no generic call tool; transaction builder emits allowlisted class only | reject |
| Agent obtains key | catastrophic | no key/seed/private-key interface in agent process | architecture violation |
| Replayed approval | duplicate recovery | single-use approval nonce, plan hash, expiry, execution uniqueness | reject |
| Duplicate recovery | double action | unique execution per approved plan + on-chain balance check immediately before send | reject/no-op |
| Token returns false / no return / fee-on-transfer | incorrect recovery accounting | safe ERC-20 wrapper + balance-delta verification; unsupported behavior policy | escalate |
| Integer/decimal bug | wrong amount | atomic integer values only; token decimals explicit; property tests | reject invariant |
| API idempotency replay | duplicate payment/claim | merchant-scoped idempotency table with request hash | return original or conflict |
| Webhook replay | merchant side duplicate | signed event ID + timestamp, merchant dedupe | merchant rejects stale/replayed |
| Signer service compromise request | unauthorized tx | signer independently loads plan/approval/policy; allowlisted contract/method/token/destination/chain | reject |
| Evidence privacy leak | user harm | object storage refs, minimal logs, scoped access, retention policy | deny/log |

## Claim authorization model

### Self-custody automatic path

1. FlowPay generates a claim challenge containing claim ID, payment ID, source chain,
   recovery destination, nonce, and expiry.
2. Customer signs with the wallet whose ownership is relevant to the source transaction.
3. FlowPay verifies the signature using chain-appropriate rules.
4. Verified wallet is bound to the claim.
5. Recovery destination is separately validated and then locked into the plan.

### Custodial/exchange path

A centralized exchange withdrawal address may not be controlled by the customer. In the initial
hackathon scope, such cases are not fully auto-recovered solely from screenshots or withdrawal
receipts. Correct behavior is `NEEDS_MORE_EVIDENCE` or `ESCALATED`.

## Signer isolation

The signer accepts a narrow request shape such as:

```text
ExecuteRecovery {
  recovery_plan_id,
  recovery_plan_hash,
  approval_id,
  expected_chain,
  expected_factory,
  expected_receiver,
  token,
  amount,
  destination,
  transaction_class
}
```

It rejects arbitrary `to/data/value` requests from the agent.

## Main known failure mode

Automatic wrong-chain recovery depends on the same approved deterministic factory protocol being
present at the expected address on the accidental EVM chain. If that infrastructure is absent or
cannot be verified, FlowPay must not attempt recovery.
