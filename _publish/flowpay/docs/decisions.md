# Architecture Decisions

## ADR-001 - Agent only at the exception boundary

**Decision:** normal payment processing is deterministic. Agent runs are created for claims or
abnormal situations requiring investigation.

**Reason:** expected-chain payment verification is better represented as explicit code and state
machines. Model variability adds risk without adding value on the happy path.

## ADR-002 - PostgreSQL outbox before Redis

**Decision:** use PostgreSQL for durable webhook/outbox scheduling initially. Add Redis only if
measured queue/latency pressure justifies it.

**Reason:** fewer failure modes and services in a hackathon system, while preserving durable retries
through `FOR UPDATE SKIP LOCKED` workers later.

## ADR-003 - EVM salt excludes chain ID

**Decision:** same logical EVM checkout uses a chain-agnostic salt, while expected chain is enforced
as payment state/policy.

**Reason:** wrong-chain recovery requires the corresponding Base/BSC address to be identical when
factory infrastructure is identical. Including chain ID in salt would make that impossible.

## ADR-004 - Factory is part of the recovery protocol

**Decision:** a chain is recoverable only after factory code hash, deployer authority, CREATE3 proxy
hash, and prediction vector are verified.

**Reason:** CREATE3 address equality depends on factory address and proxy implementation.

## ADR-005 - Automatic recovery limited to cryptographically authorized self-custody in initial scope

**Decision:** custodial/exchange-origin claims escalate unless stronger verifiable authorization is
available.

**Reason:** withdrawal receipts/screenshots do not prove control of the exchange's sending wallet.

## ADR-006 - No generic signer transaction interface

**Decision:** restricted signer accepts only persisted allowlisted recovery transaction classes bound
to a plan hash and single-use approval.

**Reason:** even a compromised agent must not be able to turn arbitrary calldata into a signed transaction.
