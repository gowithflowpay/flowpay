use crate::{MessagingError, OutboxMessage};
use rdkafka::{
    config::ClientConfig,
    producer::{FutureProducer, FutureRecord},
};
use std::time::Duration;

#[derive(Clone)]
pub struct KafkaEventPublisher {
    producer: FutureProducer,
}

impl KafkaEventPublisher {
    pub fn new(brokers: &str) -> Result<Self, MessagingError> {
        let producer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("enable.idempotence", "true")
            .create()
            .map_err(|error| MessagingError::Kafka(error.to_string()))?;
        Ok(Self { producer })
    }

    pub async fn publish(&self, message: &OutboxMessage) -> Result<(), MessagingError> {
        let payload = serde_json::to_string(&message.envelope)?;
        let key = message.envelope.event_id.to_string();
        self.producer
            .send(
                FutureRecord::to(&message.destination)
                    .key(&key)
                    .payload(&payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(error, _)| MessagingError::Kafka(error.to_string()))?;
        Ok(())
    }
}
