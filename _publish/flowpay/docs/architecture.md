# FlowPay architecture

## System boundary

FlowPay is a modular Rust backend with three process boundaries, independently deployable EVM contracts, and thin TypeScript clients.

```text
Merchant / Buyer / SDK / Telegram
              |
              v
      +-----------------+
      | FlowPay API     |
      +--------+--------+
               |
               v
          PostgreSQL
        /      |       \
       /   Transaction  \
      /      Outbox      \
     v         |          v
state/audit    v       webhook logs
          Event Relay
          /         \
         v           v
    RabbitMQ        Kafka
    commands        domain events
         |             |
         v             +--> audit / analytics / future consumers
   FlowPay Worker
         |
         +--> EVM chain adapters --> Base / BSC
         +--> model investigator
         +--> policy / simulation
         +--> restricted signer
         +--> merchant webhook delivery
```

PostgreSQL is authoritative. Brokers carry intent/facts but never replace state-machine or chain verification.

## Processes

### API server

Responsibilities:

- API-key authentication and request validation;
- idempotency keys;
- payment/claim/webhook/API-key endpoints;
- buyer checkout-facing state;
- merchant overview/read APIs;
- transactional persistence of commands/events.

The API binary does not spawn blockchain/background workers.

### Worker

Responsibilities:

- chain scanning;
- deposit verification and duplicate protection;
- canonicality/reorg revalidation;
- confirmation tracking;
- reconciliation;
- deterministic settlement;
- claim investigation execution;
- recovery simulation/execution/verification;
- merchant webhook retries.

RabbitMQ commands provide low-latency triggers. Periodic database reconciliation remains as a crash/broker-outage recovery mechanism.

### Event relay

Responsibilities:

- claim unpublished PostgreSQL outbox rows;
- publish domain events to Kafka;
- publish operational commands to RabbitMQ;
- only mark a row published after broker acknowledgement;
- retry failed publication with backoff.

## Backend modules

`domain` contains money/state/value types. `payments` contains deterministic reconciliation and checkout salt derivation. `chains` defines chain-neutral read/verification interfaces. `evm` implements EVM RPC and CREATE3 address derivation. `claims` implements authorization primitives. `agent` contains typed investigation tools and model orchestration. `policy` evaluates permitted recovery behavior. `recovery` creates canonical plans and allowlisted calls. `signer` enforces transaction classes. `webhooks` signs/encrypts merchant webhook secrets. `persistence` owns PostgreSQL representation. `messaging` owns outbox and broker adapters. `runtime` composes the modules for API/worker processes.

## Happy path

```text
POST /v1/payments
 -> validate merchant + asset + chain + amount
 -> derive payment UUID + checkout salt
 -> compute counterfactual address
 -> transactionally persist payment + payment.created + monitor command
 -> worker observes supported chain
 -> independently verifies deposit/receipt/block canonicality
 -> confirmation threshold
 -> deterministic reconciliation
 -> deterministic settlement through restricted signer
 -> receipt verification
 -> COMPLETED
 -> signed merchant webhook
```

No model is invoked.

## Exception path

```text
claim created
 -> wallet authorization where applicable
 -> claim.investigate command
 -> model selects read-only typed tools
 -> constrained disposition
 -> deterministic evidence gate
 -> RecoveryPolicy
 -> canonical RecoveryPlan + hash
 -> eth_call simulation + gas analysis
 -> merchant/human approval
 -> recovery.execute command
 -> approval atomically consumed
 -> restricted signer
 -> receipt + balance-delta verification
 -> RECOVERED
```

If facts remain ambiguous, FlowPay escalates rather than guessing.

## CREATE3 deployment invariant

The checkout address is derived from the factory, salt and fixed proxy creation-code hash. Same logical checkout addresses on Base/BSC are only asserted if the factory address and proxy hash match on both chains. Local deployment aborts if the factory invariant fails.

## Solana

Solana is intentionally not represented as an EVM adapter. It remains a separate future adapter with Solana-native payment/recovery semantics.
