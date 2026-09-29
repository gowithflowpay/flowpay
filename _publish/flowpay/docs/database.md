# Database Design

The canonical schema is `backend/database/migrations/0001_init.sql`.

## Principles

- PostgreSQL 16.
- UUID internal primary keys; public IDs are separate strings.
- Token/native monetary values use `NUMERIC(78,0)`, enough for unsigned 256-bit atomic units.
- No floating-point columns for money.
- Every payment/claim has a monotonic `version` for concurrent update protection.
- Chain events are idempotent through unique transaction/log constraints.
- State transitions, agent tool calls, decisions, approvals, executions, and audit logs are append-only records.
- Evidence is explicitly marked non-authoritative; it can inform investigation but cannot directly prove chain facts.

## Core relationship summary

```text
merchant
  -> api_keys
  -> payments
      -> checkout_addresses
      -> deposits -> chain_transactions
      -> payment_state_transitions
      -> settlements
      -> claims
          -> claim_evidence
          -> claim_wallet_signatures
          -> recovery_plans
              -> approvals
              -> recovery_executions
          -> agent_runs
              -> agent_tool_calls
              -> agent_decisions
  -> webhook_endpoints
  -> webhook_events -> webhook_deliveries

audit_logs links request/payment/claim/agent IDs without storing secrets.
```

## Important constraints

- `checkout_addresses`: unique `(chain,address)` and unique `(payment_id,chain)`.
- `deposits`: unique event identity prevents duplicate provider/webhook processing.
- `claims`: active claimed transaction dedupe prevents duplicate recovery claims for the same payment/chain/tx.
- `approvals`: plan hash and approval nonce are unique; status supports single-use consumption.
- `recovery_executions`: one execution per recovery plan and one per approval.
- `claim_evidence.authoritative` is constrained to `false` to prevent code paths from silently treating uploads as financial truth.
