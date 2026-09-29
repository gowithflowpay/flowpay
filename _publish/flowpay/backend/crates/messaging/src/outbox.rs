use crate::{MessageEnvelope, MessageKind, MessagingError};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct OutboxMessage {
    pub id: Uuid,
    pub kind: MessageKind,
    pub destination: String,
    pub routing_key: Option<String>,
    pub envelope: MessageEnvelope,
    pub attempts: i32,
}

#[derive(Clone)]
pub struct OutboxStore {
    pool: PgPool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InboxReservation {
    Acquired,
    Completed,
    Busy,
    Exhausted,
}

impl OutboxStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn claim_batch(
        &self,
        relay_id: &str,
        limit: i64,
    ) -> Result<Vec<OutboxMessage>, MessagingError> {
        let rows = sqlx::query(
            r#"
            WITH picked AS (
                SELECT id
                FROM outbox_messages
                WHERE published_at IS NULL
                  AND available_at <= now()
                  AND (locked_at IS NULL OR locked_at < now() - interval '30 seconds')
                ORDER BY occurred_at, id
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE outbox_messages o
            SET locked_at = now(), locked_by = $2, attempts = attempts + 1
            FROM picked
            WHERE o.id = picked.id
            RETURNING o.id,o.message_kind,o.destination,o.routing_key,o.event_id,o.event_type,
                      o.event_version,o.aggregate_type,o.aggregate_id,o.occurred_at,o.correlation_id,
                      o.causation_id,o.payload,o.attempts
            "#,
        )
        .bind(limit)
        .bind(relay_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_message).collect()
    }

    pub async fn mark_published(&self, id: Uuid) -> Result<(), MessagingError> {
        sqlx::query(
            "UPDATE outbox_messages SET published_at=now(),locked_at=NULL,locked_by=NULL,last_error=NULL WHERE id=$1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_failed(&self, id: Uuid, error: &str) -> Result<(), MessagingError> {
        sqlx::query(
            r#"
            UPDATE outbox_messages
            SET locked_at=NULL,
                locked_by=NULL,
                last_error=$2,
                available_at=now() + make_interval(secs => LEAST(300, GREATEST(1, attempts * attempts)))
            WHERE id=$1
            "#,
        )
        .bind(id)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn reserve_message(
        &self,
        consumer_name: &str,
        event_id: Uuid,
        stale_after_seconds: i64,
        max_attempts: i32,
    ) -> Result<InboxReservation, MessagingError> {
        let acquired = sqlx::query_scalar::<_, String>(
            r#"
            INSERT INTO processed_messages(
                consumer_name,event_id,status,processing_started_at,attempt_count,next_attempt_at
            ) VALUES($1,$2,'PROCESSING',now(),1,now())
            ON CONFLICT (consumer_name,event_id) DO UPDATE
            SET status='PROCESSING',
                processing_started_at=now(),
                failed_at=NULL,
                last_error=NULL,
                attempt_count=processed_messages.attempt_count + 1
            WHERE processed_messages.status='FAILED'
                  AND processed_messages.next_attempt_at <= now()
                  AND processed_messages.attempt_count < $4
               OR processed_messages.status='PROCESSING'
                  AND processed_messages.processing_started_at < now() - make_interval(secs => $3)
                  AND processed_messages.attempt_count < $4
            RETURNING status
            "#,
        )
        .bind(consumer_name)
        .bind(event_id)
        .bind(stale_after_seconds)
        .bind(max_attempts)
        .fetch_optional(&self.pool)
        .await?;
        if acquired.is_some() {
            return Ok(InboxReservation::Acquired);
        }

        let row = sqlx::query("SELECT status,attempt_count FROM processed_messages WHERE consumer_name=$1 AND event_id=$2")
            .bind(consumer_name)
            .bind(event_id)
            .fetch_one(&self.pool)
            .await?;
        let status: String = row.try_get("status")?;
        let attempts: i32 = row.try_get("attempt_count")?;
        Ok(match status.as_str() {
            "COMPLETED" => InboxReservation::Completed,
            "FAILED" if attempts >= max_attempts => InboxReservation::Exhausted,
            _ => InboxReservation::Busy,
        })
    }

    pub async fn complete_message(
        &self,
        consumer_name: &str,
        event_id: Uuid,
    ) -> Result<(), MessagingError> {
        sqlx::query(
            "UPDATE processed_messages SET status='COMPLETED',completed_at=now(),processed_at=now(),last_error=NULL WHERE consumer_name=$1 AND event_id=$2 AND status='PROCESSING'",
        )
        .bind(consumer_name)
        .bind(event_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn fail_message(
        &self,
        consumer_name: &str,
        event_id: Uuid,
        error: &str,
    ) -> Result<(), MessagingError> {
        sqlx::query(
            r#"
            UPDATE processed_messages
            SET status='FAILED',failed_at=now(),last_error=$3,
                next_attempt_at=now() + make_interval(secs => LEAST(60, attempt_count * attempt_count))
            WHERE consumer_name=$1 AND event_id=$2 AND status='PROCESSING'
            "#,
        )
        .bind(consumer_name)
        .bind(event_id)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_domain_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    topic: &str,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    payload: Value,
    correlation_id: Option<&str>,
    causation_id: Option<Uuid>,
) -> Result<Uuid, MessagingError> {
    enqueue_tx(
        tx,
        MessageKind::DomainEvent,
        topic,
        None,
        event_type,
        aggregate_type,
        aggregate_id,
        payload,
        correlation_id,
        causation_id,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_command_at_tx(
    tx: &mut Transaction<'_, Postgres>,
    routing_key: &str,
    command_type: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    payload: Value,
    correlation_id: Option<&str>,
    causation_id: Option<Uuid>,
    available_at: OffsetDateTime,
) -> Result<Uuid, MessagingError> {
    let event_id = enqueue_command_tx(
        tx,
        routing_key,
        command_type,
        aggregate_type,
        aggregate_id,
        payload,
        correlation_id,
        causation_id,
    )
    .await?;
    sqlx::query("UPDATE outbox_messages SET available_at=$2 WHERE event_id=$1")
        .bind(event_id)
        .bind(available_at)
        .execute(&mut **tx)
        .await?;
    Ok(event_id)
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_command_tx(
    tx: &mut Transaction<'_, Postgres>,
    routing_key: &str,
    command_type: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    payload: Value,
    correlation_id: Option<&str>,
    causation_id: Option<Uuid>,
) -> Result<Uuid, MessagingError> {
    enqueue_tx(
        tx,
        MessageKind::Command,
        "flowpay.commands",
        Some(routing_key),
        command_type,
        aggregate_type,
        aggregate_id,
        payload,
        correlation_id,
        causation_id,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn enqueue_tx(
    tx: &mut Transaction<'_, Postgres>,
    kind: MessageKind,
    destination: &str,
    routing_key: Option<&str>,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    payload: Value,
    correlation_id: Option<&str>,
    causation_id: Option<Uuid>,
) -> Result<Uuid, MessagingError> {
    let event_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO outbox_messages(
            id,message_kind,destination,routing_key,event_id,event_type,event_version,
            aggregate_type,aggregate_id,correlation_id,causation_id,payload
        ) VALUES($1,$2,$3,$4,$5,$6,1,$7,$8,$9,$10,$11)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(kind.as_db_str())
    .bind(destination)
    .bind(routing_key)
    .bind(event_id)
    .bind(event_type)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(correlation_id)
    .bind(causation_id)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(event_id)
}

fn row_to_message(row: sqlx::postgres::PgRow) -> Result<OutboxMessage, MessagingError> {
    let kind = match row.try_get::<&str, _>("message_kind")? {
        "DOMAIN_EVENT" => MessageKind::DomainEvent,
        "COMMAND" => MessageKind::Command,
        other => {
            return Err(MessagingError::Database(sqlx::Error::Protocol(format!(
                "unknown outbox message kind {other}"
            ))))
        }
    };
    let version = u16::try_from(row.try_get::<i32, _>("event_version")?).map_err(|_| {
        MessagingError::Database(sqlx::Error::Protocol("invalid outbox event version".into()))
    })?;
    Ok(OutboxMessage {
        id: row.try_get("id")?,
        kind,
        destination: row.try_get("destination")?,
        routing_key: row.try_get("routing_key")?,
        envelope: MessageEnvelope {
            event_id: row.try_get("event_id")?,
            event_type: row.try_get("event_type")?,
            version,
            aggregate_type: row.try_get("aggregate_type")?,
            aggregate_id: row.try_get("aggregate_id")?,
            occurred_at: row.try_get::<OffsetDateTime, _>("occurred_at")?,
            correlation_id: row.try_get("correlation_id")?,
            causation_id: row.try_get("causation_id")?,
            payload: row.try_get("payload")?,
        },
        attempts: row.try_get("attempts")?,
    })
}
