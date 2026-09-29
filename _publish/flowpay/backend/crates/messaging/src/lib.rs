mod envelope;
mod kafka;
mod outbox;
mod rabbitmq;

pub use envelope::{MessageEnvelope, MessageKind};
pub use kafka::KafkaEventPublisher;
pub use outbox::{
    enqueue_command_at_tx, enqueue_command_tx, enqueue_domain_event_tx, InboxReservation,
    OutboxMessage, OutboxStore,
};
pub use rabbitmq::{RabbitCommandConsumer, RabbitCommandPublisher};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessagingError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("kafka error: {0}")]
    Kafka(String),
    #[error("rabbitmq error: {0}")]
    Rabbit(String),
}
