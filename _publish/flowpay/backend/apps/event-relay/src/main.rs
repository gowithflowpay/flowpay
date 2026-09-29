use flowpay_messaging::{KafkaEventPublisher, MessageKind, OutboxStore, RabbitCommandPublisher};
use sqlx::postgres::PgPoolOptions;
use std::{env, time::Duration};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,flowpay=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let database_url = env::var("DATABASE_URL")?;
    let kafka_brokers = env::var("KAFKA_BROKERS").unwrap_or_else(|_| "kafka:9092".into());
    let rabbitmq_url =
        env::var("RABBITMQ_URL").unwrap_or_else(|_| "amqp://guest:guest@rabbitmq:5672/%2f".into());
    let relay_id = format!("relay-{}", uuid::Uuid::now_v7());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    let store = OutboxStore::new(pool);
    let mut kafka: Option<KafkaEventPublisher> = None;
    let mut rabbit: Option<RabbitCommandPublisher> = None;
    let mut consecutive_failures = 0_u32;
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = shutdown_tx.send(true);
        }
    });

    info!(%relay_id, "FlowPay event relay started");
    loop {
        if *shutdown_rx.borrow() {
            break;
        }
        if kafka.is_none() {
            match KafkaEventPublisher::new(&kafka_brokers) {
                Ok(publisher) => kafka = Some(publisher),
                Err(error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    warn!(%error, "Kafka producer initialization failed; retrying");
                    wait_for_retry(consecutive_failures, &mut shutdown_rx).await;
                    continue;
                }
            }
        }
        if rabbit.is_none() {
            match RabbitCommandPublisher::connect(&rabbitmq_url).await {
                Ok(publisher) => rabbit = Some(publisher),
                Err(error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    warn!(%error, "RabbitMQ connection failed; retrying");
                    wait_for_retry(consecutive_failures, &mut shutdown_rx).await;
                    continue;
                }
            }
        }

        let batch = store.claim_batch(&relay_id, 100).await?;
        if batch.is_empty() {
            tokio::select! {
                () = tokio::time::sleep(Duration::from_millis(250)) => {}
                result = shutdown_rx.changed() => {
                    if result.is_err() || *shutdown_rx.borrow() { break; }
                }
            }
            continue;
        }
        let mut publish_failed = false;
        for message in batch {
            let result = match message.kind {
                MessageKind::DomainEvent => {
                    kafka
                        .as_ref()
                        .expect("Kafka publisher initialized")
                        .publish(&message)
                        .await
                }
                MessageKind::Command => {
                    rabbit
                        .as_ref()
                        .expect("RabbitMQ publisher initialized")
                        .publish(&message)
                        .await
                }
            };
            match result {
                Ok(()) => {
                    store.mark_published(message.id).await?;
                    consecutive_failures = 0;
                }
                Err(err) => {
                    error!(event_id=%message.envelope.event_id,error=%err,"outbox publish failed; preserving and rescheduling message");
                    store.mark_failed(message.id, &err.to_string()).await?;
                    match message.kind {
                        MessageKind::DomainEvent => kafka = None,
                        MessageKind::Command => rabbit = None,
                    }
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    publish_failed = true;
                    break;
                }
            }
        }
        if publish_failed {
            wait_for_retry(consecutive_failures, &mut shutdown_rx).await;
        }
    }
    info!(%relay_id, "FlowPay event relay shut down cleanly");
    Ok(())
}

async fn wait_for_retry(failures: u32, shutdown: &mut tokio::sync::watch::Receiver<bool>) {
    let exponent = failures.saturating_sub(1).min(5);
    let base_ms = 500_u64.saturating_mul(1_u64 << exponent);
    let jitter_ms = u64::from(uuid::Uuid::now_v7().as_bytes()[15]) * 2;
    let delay = Duration::from_millis((base_ms + jitter_ms).min(30_000));
    tokio::select! {
        () = tokio::time::sleep(delay) => {}
        _ = shutdown.changed() => {}
    }
}
