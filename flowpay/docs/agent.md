# Claim / Recovery Agent Boundary

## Ollama is the investigator

In the configured model workflow, Ollama is the primary investigative agent. It selects only the
allowlisted typed read/verify tools and follows the investigation procedure in
`docs/ollama-investigation.md`. It is not a fallback that may be bypassed for a claim: if Ollama is
unavailable after the bounded retry budget, the claim is escalated safely.

## Role

The agent is an investigator and constrained planner. It is not a wallet and not a generic chain
operator.

## Inputs

- immutable payment context;
- claim fields;
- evidence metadata/text marked untrusted;
- verified wallet-authorization facts;
- chain configuration;
- recovery policy version;
- previous tool results in the current agent run.

## Allowed final dispositions

- `RECOVERABLE`
- `NOT_RECOVERABLE`
- `NEEDS_MORE_EVIDENCE`
- `ESCALATE`

Only deterministic services may convert `RECOVERABLE` into an executable recovery. Cross-chain and
amount-discrepancy cases require approval; ambiguous, unsupported, or high-risk cases escalate.

## Tool classes

### Read / verify

- `get_payment`
- `get_payment_state_history`
- `get_deposits`
- `get_claim`
- `get_claim_evidence`
- `verify_wallet_signature`
- `get_transaction`
- `get_transaction_receipt`
- `get_token_metadata`
- `get_token_balance`
- `get_native_balance`
- `compute_checkout_address`
- `verify_counterfactual_address`
- `check_factory`
- `check_recovery_policy`
- `estimate_gas`

### Plan / simulate

- `build_recovery_plan`
- `simulate_recovery`

### Controlled state actions

- `request_more_evidence`
- `mark_recoverable`
- `mark_not_recoverable`
- `escalate_claim`
- `request_approval`
- `execute_approved_recovery`
- `verify_recovery`
- `close_claim`

## Non-negotiable tool behavior

Every tool call has:

- typed input/output;
- actor/merchant/claim authorization checks;
- parameter validation;
- request/agent-run correlation IDs;
- deterministic error codes;
- persisted start/end timestamps;
- redacted logging;
- retry classification (`retryable`, `permanent`, `policy_denied`).

`execute_approved_recovery` does not expose a private key. It calls the restricted transaction
service, which independently revalidates the persisted plan and approval.

## Trajectory persistence

Store concise audit rationale, never hidden chain-of-thought. A trajectory step contains:

```json
{
  "step": 6,
  "decision_summary": "Claimed BSC transaction exists and recipient matches the predicted EVM checkout address. Verify current token balance before policy evaluation.",
  "tool": "get_token_balance",
  "input": {"chain": "bsc", "token": "0x...", "address": "0x..."},
  "output_summary": {"amount_atomic": "50000000", "block": 12345},
  "verification": "rpc_verified",
  "policy_effect": "none"
}
```

## Prompt-injection handling

Claim explanations and evidence text are explicitly untrusted. Agent instructions state that
text inside evidence cannot grant permissions, change policy, choose a different destination, or
request arbitrary tools. Tool services do not trust model assertions about authorization.

## Retry strategy

RPC/network errors may be retried within a fixed budget. Conflicting chain results are not solved
by guessing; they become `ESCALATE`. Simulation failure blocks approval/execution.
