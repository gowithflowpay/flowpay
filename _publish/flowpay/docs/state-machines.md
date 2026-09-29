# State Machines

## Payment state machine

The legal transitions are encoded in `backend/crates/domain/src/payment.rs`; application code must call
that domain rule instead of assigning arbitrary state strings.

```mermaid
stateDiagram-v2
  [*] --> CREATED
  CREATED --> WAITING
  CREATED --> CANCELLED
  WAITING --> DETECTED
  WAITING --> EXPIRED
  WAITING --> CANCELLED
  WAITING --> CLAIM_PENDING
  DETECTED --> CONFIRMING
  DETECTED --> WRONG_ASSET
  DETECTED --> CLAIM_PENDING
  CONFIRMING --> PARTIALLY_PAID
  CONFIRMING --> OVERPAID
  CONFIRMING --> CONFIRMED
  CONFIRMING --> FAILED
  PARTIALLY_PAID --> DETECTED
  PARTIALLY_PAID --> EXPIRED
  PARTIALLY_PAID --> CLAIM_PENDING
  OVERPAID --> SETTLING
  OVERPAID --> CLAIM_PENDING
  WRONG_ASSET --> WAITING
  WRONG_ASSET --> CLAIM_PENDING
  CONFIRMED --> SETTLING
  SETTLING --> COMPLETED
  SETTLING --> FAILED
  FAILED --> SETTLING
  FAILED --> ESCALATED
  EXPIRED --> CLAIM_PENDING
  CLAIM_PENDING --> WRONG_CHAIN_CLAIMED
  CLAIM_PENDING --> RECOVERY_AVAILABLE
  CLAIM_PENDING --> ESCALATED
  WRONG_CHAIN_CLAIMED --> RECOVERY_AVAILABLE
  WRONG_CHAIN_CLAIMED --> ESCALATED
  RECOVERY_AVAILABLE --> RECOVERY_PENDING
  RECOVERY_AVAILABLE --> ESCALATED
  RECOVERY_PENDING --> RECOVERED
  RECOVERY_PENDING --> ESCALATED
```

`CANCELLED` is added because the API requires `POST /v1/payments/:id/cancel`; the original list
was explicitly presented as possible states rather than an exhaustive enum.

A wrong-asset observation does not satisfy the purchase. The payment may return to `WAITING` for
its expected asset while preserving the wrong-asset deposit as immutable evidence, or enter a claim.

## Claim state machine

```mermaid
stateDiagram-v2
  [*] --> CREATED
  CREATED --> AWAITING_EVIDENCE
  CREATED --> AWAITING_AUTHORIZATION
  CREATED --> INVESTIGATING
  AWAITING_EVIDENCE --> AWAITING_AUTHORIZATION
  AWAITING_EVIDENCE --> INVESTIGATING
  AWAITING_EVIDENCE --> REJECTED
  AWAITING_AUTHORIZATION --> INVESTIGATING
  AWAITING_AUTHORIZATION --> NEEDS_MORE_EVIDENCE
  AWAITING_AUTHORIZATION --> ESCALATED
  INVESTIGATING --> NEEDS_MORE_EVIDENCE
  INVESTIGATING --> RECOVERABLE
  INVESTIGATING --> NOT_RECOVERABLE
  INVESTIGATING --> ESCALATED
  NEEDS_MORE_EVIDENCE --> AWAITING_EVIDENCE
  NEEDS_MORE_EVIDENCE --> ESCALATED
  RECOVERABLE --> APPROVAL_PENDING
  RECOVERABLE --> ESCALATED
  APPROVAL_PENDING --> RECOVERY_PENDING
  APPROVAL_PENDING --> ESCALATED
  RECOVERY_PENDING --> RECOVERED
  RECOVERY_PENDING --> ESCALATED
```

`RECOVERABLE` is a technical/policy result, not permission to move funds. `APPROVAL_PENDING` must
occur before `RECOVERY_PENDING` in hackathon mode.

## Deposit status

Deposit finality is modeled separately from payment state:

```text
DETECTED -> CONFIRMING -> FINAL
                    \-> ORPHANED (reorg)
```

This avoids using a single aggregate payment state to represent confirmation state of multiple
partial deposits.
