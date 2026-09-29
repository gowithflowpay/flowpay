# Messaging architecture

## Goals

Messaging must reduce process coupling without becoming the source of financial truth. PostgreSQL remains authoritative. Messages are assumed to be delivered at least once.

## Transactional outbox

A state-changing database transaction may insert one or more rows into `outbox_messages` before commit. If the transaction rolls back, the publication intent rolls back with it.

Example payment creation:

```text
BEGIN
  INSERT payment
  INSERT checkout address
  INSERT state transition
  INSERT outbox: payment.created -> Kafka
  INSERT outbox: payment.monitor.start -> RabbitMQ
COMMIT
```

The event relay claims unpublished rows using `FOR UPDATE SKIP LOCKED`, publishes them, and only then stamps `published_at`. Failed publications are unlocked and rescheduled with bounded backoff.

## Domain events: Kafka

Kafka represents facts that already happened. Examples:

```text
payment.created
payment.detected
payment.partially_paid
payment.confirmed
payment.completed
payment.failed

claim.created
claim.investigation.started
claim.recoverable
claim.recovery_pending
claim.recovered
claim.escalated

recovery.approved
```

Events use an envelope with:

```json
{
  "event_id": "uuid-v7",
  "event_type": "payment.completed",
  "version": 1,
  "aggregate_type": "PAYMENT",
  "aggregate_id": "pay_...",
  "occurred_at": "...",
  "correlation_id": "...",
  "causation_id": "...",
  "payload": {}
}
```

Consumer groups own their own offsets. One consumer processing an event does not delete it for other consumers.

## Operational commands: RabbitMQ

RabbitMQ represents work one worker should perform. Current routing keys include:

```text
payment.monitor.start
payment.reconcile
payment.settlement.execute
claim.investigate
recovery.simulate
recovery.execute
recovery.verify
webhook.deliver
webhook.retry
```

The worker queue is durable and quorum-backed. Messages are persistent. Consumers record completed event IDs in `processed_messages`; duplicate deliveries therefore ACK without repeating a financial action.

Failed commands are negatively acknowledged and redelivered up to the queue delivery limit. Repeatedly failing commands are routed to `flowpay.commands.worker.dlq` through `flowpay.commands.dlx`.

## Why periodic DB reconciliation still exists

A broker should improve latency, not become a correctness dependency. Chain monitoring, settlement recovery, claim investigation recovery, and webhook retries retain periodic database reconciliation loops. This means:

- a RabbitMQ outage does not lose a payment;
- a command published twice does not execute settlement twice;
- a worker crash before ACK causes a safe redelivery;
- PostgreSQL state can reconstruct pending work.

For the hackathon build, settlement, claim/recovery orchestration, and webhook delivery also take short PostgreSQL service leases. This prevents two worker replicas from concurrently submitting the same consequential operation while retaining broker-triggered low latency. The leases expire automatically after a crashed worker.

## In-process events

Tokio tasks/channels may coordinate code inside one process but are never durable and never treated as a source of truth.

## Merchant webhooks

Merchant webhooks retain their own signed delivery log and retry schedule in PostgreSQL. During the hackathon they are not made dependent on Kafka because that would introduce two competing sources of merchant-delivery truth during migration. Kafka remains available for audit, analytics, future independent services, and event-driven consumers.
