# FlowPay

**Crypto payments for anyone who can build.**

FlowPay is an agentic crypto payment-operations and recovery layer for permissionless commerce. Ordinary payments remain deterministic. A model-driven investigator is invoked only when a payment becomes exceptional, and every consequential recovery action is still verified by deterministic policy, simulation, human approval, and a restricted signer.

## Status

The repository contains the EVM payment engine, CREATE3-style counterfactual checkout contracts, claims/recovery workflow, model-driven investigation layer, API/worker split, PostgreSQL persistence, transactional outbox, RabbitMQ operational commands, Kafka domain events, merchant dashboard, hosted checkout/claim UI, Node SDK, Telegram example, spec-regression evaluator, and a real-system dual-Anvil E2E evaluator.

Do not present old fixture metrics as end-to-end system results. See `docs/evaluation.md` for the evidence boundary.

## Architecture principle

```text
Normal payment
    -> deterministic payment engine

Abnormal payment / claim
    -> model-driven investigation using read-only typed tools
    -> deterministic evidence gate
    -> RecoveryPolicy
    -> canonical RecoveryPlan
    -> simulation
    -> human approval
    -> restricted signer
    -> blockchain
```

The model never receives private keys and has no arbitrary signing, transaction-building, calldata, or unrestricted RPC tool.

## Repository layout

```text
flowpay/
├── backend/                     # FlowPay itself
│   ├── Cargo.toml
│   ├── apps/
│   │   ├── api-server/         # HTTP/API process
│   │   ├── worker/             # chain/payment/claim/recovery workers
│   │   └── event-relay/        # PostgreSQL outbox -> Kafka/RabbitMQ
│   ├── crates/
│   │   ├── runtime/            # shared API/worker composition
│   │   ├── domain/
│   │   ├── payments/
│   │   ├── chains/
│   │   ├── evm/
│   │   ├── claims/
│   │   ├── recovery/
│   │   ├── agent/
│   │   ├── policy/
│   │   ├── signer/
│   │   ├── webhooks/
│   │   ├── persistence/
│   │   └── messaging/
│   └── database/migrations/
├── contracts/                   # independently deployable EVM infrastructure
├── apps/
│   ├── merchant/                # merchant client
│   └── checkout/                # buyer checkout + recovery claim client
├── sdk/
│   ├── node/
│   └── telegram/
├── evals/                       # spec + real-system evaluation
├── infra/                       # local-chain and RPC-proxy infrastructure
├── docs/
├── scripts/
├── Makefile
└── .env.example
```

The frontends are intentionally thin. Payment state, chain verification, reconciliation, recovery decisions, simulation, approval semantics, and signing authority live in the backend.

## Backend processes

`flowpay-api-server` owns HTTP authentication, validation, idempotency, checkout/claim APIs, merchant APIs, and persistence-facing commands. It does **not** spawn background workers.

`flowpay-worker` owns chain monitoring, canonicality/confirmation checks, reconciliation, deterministic settlement, model investigation runs, recovery execution/verification, and merchant webhook retries.

`flowpay-event-relay` owns the transactional outbox. It publishes domain events to Kafka and operational commands to RabbitMQ only after the originating database transaction commits.

## Messaging model

FlowPay deliberately does not use RabbitMQ, Kafka, and in-process events for the same purpose.

- **PostgreSQL** is the source of truth.
- **Transactional outbox** makes state changes and publication intent atomic.
- **RabbitMQ** carries work that a worker should perform, e.g. `claim.investigate` and `recovery.execute`.
- **Kafka** carries durable facts, e.g. `payment.created`, `payment.completed`, `claim.recoverable`, and `recovery.approved`.
- **In-process Tokio tasks/channels** are process-local implementation details only.

RabbitMQ consumers are idempotent through `processed_messages`. The worker queue is durable, uses a bounded delivery limit, and dead-letters repeatedly failing commands. Consequential worker loops also use short PostgreSQL coordinator leases to avoid cross-replica duplicate settlement/recovery/webhook execution. Kafka messages use stable event IDs, aggregate IDs, versioning, correlation/causation metadata, and JSON payloads.

See `docs/messaging.md`.

## CREATE3 / counterfactual checkout architecture

FlowPay uses a fixed `CheckoutProxy` deployed with CREATE2. The proxy's first CREATE deploys an extremely small `CheckoutReceiver`. The final receiver address is therefore computable before the receiver exists.

Cross-chain address equality is **not** assumed merely because CREATE3 is used. It is valid only when the EVM chains share:

1. the same FlowPay factory address;
2. the same checkout salt;
3. identical proxy creation code/hash; and
4. compatible CREATE2/CREATE semantics.

The checkout salt is bound to immutable merchant/payment identifiers and deliberately excludes EVM chain ID. Expected chain remains a separate payment invariant, which is what makes a recoverable Base-vs-BSC mistake possible without accepting the BSC transfer as payment for the Base invoice.

The local deployment bootstrap deploys the factory from the same deployer/nonce on both Anvil chains and aborts if the factory addresses differ.

## Agent boundary

`backend/crates/agent/src/model.rs` implements the model-driven investigator. The model can choose only typed investigation tools. A recoverable recommendation is still insufficient to move money: Rust rechecks the relevant evidence and builds the only allowed recovery plan class before policy, simulation, approval, signer authorization, and receipt/balance verification.

Ambiguous ownership, unsupported networks/tokens, failed transaction verification, missing balance, counterfactual mismatch, factory mismatch, and simulation failure prefer escalation over guessing.

## API

Core routes include:

```text
POST /v1/payments
GET  /v1/payments
GET  /v1/payments/:id
POST /v1/payments/:id/cancel
GET  /v1/payments/:id/deposits

POST /v1/claims
GET  /v1/claims
GET  /v1/claims/:id
POST /v1/claims/:id/evidence
POST /v1/claims/:id/authorize
POST /v1/claims/:id/fund
POST /v1/claims/:id/approve

GET/POST /v1/webhooks
POST     /v1/webhooks/test
GET/POST /v1/api-keys
POST     /v1/api-keys/:id/revoke
GET      /v1/logs
GET      /v1/merchant/overview
GET      /health
```

The older `/v1/overview` alias remains for compatibility.

## Local environment

Run PostgreSQL, RabbitMQ, and Kafka as native services, then prepare FlowPay:

```bash
cp .env.example .env
./scripts/bootstrap-local.sh
make dev
```

The bootstrap starts both Anvil chains, deploys contracts, applies migrations, and seeds local data. `make dev` starts the API, worker, event relay, merchant UI, and checkout UI.

To start only the complete backend (API, worker, and event relay) with one command:

```bash
make backend
```

PostgreSQL, RabbitMQ, and Kafka must already be running as native services. On the first local run, the command also bootstraps the two Anvil chains, contracts, migrations, and seed data. Logs are written under `runtime/`; Ctrl-C stops all three backend processes.

Local endpoints:

```text
Merchant UI  http://localhost:3000
Checkout UI  http://localhost:3001
FlowPay API  http://localhost:8080
RabbitMQ UI  http://localhost:15672
Base RPC     http://localhost:8545
BSC RPC      http://localhost:9545
Kafka        localhost:9092
```

For the configured local or testnet environment, start infrastructure, migrations, the API,
worker, relay, both web apps, agent mode, and ngrok with one command:

```bash
make start
```

Startup verifies identical deployed factory bytecode and CREATE3 checkout prediction across every
configured EVM recovery network before accepting payments. Logs are written under `runtime/`.

Model mode uses Ollama exclusively with `qwen2.5-coder:7b`. FlowPay does not switch investigative providers; if Ollama is unavailable, bounded retries end in safe claim escalation.

## Host-mode commands

```bash
cargo test --manifest-path backend/Cargo.toml --workspace
(cd contracts && forge test)
./scripts/reset-local.sh
./scripts/run-event-relay.sh
./scripts/run-worker.sh model
./scripts/run-api.sh model
```

See `docs/reproduction.md` for the complete clean-machine flow.

## Evaluation

There are two distinct evaluators:

- `evals/runner/` is a deterministic **spec regression** suite. It is useful for checking expected behavior but does not prove the deployed system.
- `evals/e2e/run.mjs` drives the real API, PostgreSQL, contracts, two Anvil chains, worker, webhooks, and configured model. This is the evaluator whose observed results may be presented as system performance.

The legacy fixture run produced 35% baseline vs 80% constrained-workflow autonomous resolution with 0% unsafe actions, but those numbers are intentionally labeled as fixture/spec results rather than authoritative end-to-end evidence.

Run the real evaluator with:

```bash
./scripts/run-e2e-evals.sh all
```

Model mode requires the configured local Ollama model. Ollama is the only supported investigative provider.

## Frontends

`apps/merchant` is the merchant-facing dashboard. It renders backend-owned balances, payment/claim state, model/tool provenance, simulation, approval, recovery execution, API keys, webhooks, and logs.

`apps/checkout` is the buyer-facing hosted checkout. It renders the expected amount/network/address, QR, expiry and live state, then provides the four-stage claim/authorization flow and safe investigation status without exposing hidden model reasoning.

## Network scope

- Base/local EVM: implemented first.
- BSC/local EVM: shares the EVM adapter and powers wrong-chain recovery evaluation.
- Solana: architecturally separate and explicitly unsupported in this build rather than emulated with EVM assumptions.

## Security summary

- no private keys in model context;
- no arbitrary transaction tool for the model;
- atomic integer token amounts only;
- independent chain verification before critical state changes;
- merchant webhooks signed and retried idempotently;
- self-custody recovery requires wallet authorization;
- recovery destination is plan-bound;
- simulation precedes approval;
- approval is single-use and plan-hash-bound;
- restricted signer only accepts allowlisted transaction classes;
- recovery verifies receipt and destination balance change;
- uploaded evidence is investigative input, never financial truth.

## Known limitations

- Public hosted checkout access still uses a server-side merchant credential boundary; production should issue scoped, expiring checkout-session credentials.
- Solana is not implemented.
- Current verification status is recorded in the evaluation artifacts and changelog; absence of an artifact is not a passing result.
- Kafka currently provides the durable domain-event stream; merchant webhook delivery remains backed by its dedicated PostgreSQL delivery log/retry worker rather than being dependent on Kafka, avoiding two competing delivery sources during the hackathon.

## Documentation

Start with `docs/architecture.md`, `docs/threat-model.md`, `docs/create3.md`, `docs/agent.md`, `docs/messaging.md`, `docs/evaluation.md`, and `docs/reproduction.md`.
