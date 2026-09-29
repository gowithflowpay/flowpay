use crate::{MessageEnvelope, MessagingError, OutboxMessage};
use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
    },
    types::{AMQPValue, FieldTable},
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};

pub struct RabbitCommandPublisher {
    _connection: Connection,
    channel: Channel,
}

impl RabbitCommandPublisher {
    pub async fn connect(uri: &str) -> Result<Self, MessagingError> {
        let connection = Connection::connect(uri, ConnectionProperties::default())
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        let channel = connection
            .create_channel()
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        channel
            .exchange_declare(
                "flowpay.commands",
                ExchangeKind::Direct,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        Ok(Self {
            _connection: connection,
            channel,
        })
    }

    pub async fn publish(&self, message: &OutboxMessage) -> Result<(), MessagingError> {
        let routing_key = message
            .routing_key
            .as_deref()
            .ok_or_else(|| MessagingError::Rabbit("command is missing routing key".into()))?;
        let payload = serde_json::to_vec(&message.envelope)?;
        self.channel
            .basic_publish(
                "flowpay.commands",
                routing_key,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default()
                    .with_content_type("application/json".into())
                    .with_delivery_mode(2),
            )
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        Ok(())
    }
}

pub struct RabbitCommandConsumer {
    _connection: Connection,
    channel: Channel,
}

impl RabbitCommandConsumer {
    pub async fn connect(uri: &str) -> Result<Self, MessagingError> {
        let connection = Connection::connect(uri, ConnectionProperties::default())
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        let channel = connection
            .create_channel()
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        channel
            .exchange_declare(
                "flowpay.commands",
                ExchangeKind::Direct,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        channel
            .exchange_declare(
                "flowpay.commands.dlx",
                ExchangeKind::Fanout,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        Ok(Self {
            _connection: connection,
            channel,
        })
    }

    pub async fn run<F, Fut>(
        self,
        queue: &str,
        routing_keys: &[&str],
        mut handler: F,
    ) -> Result<(), MessagingError>
    where
        F: FnMut(MessageEnvelope) -> Fut,
        Fut: std::future::Future<Output = Result<(), MessagingError>>,
    {
        let mut queue_args = FieldTable::default();
        queue_args.insert(
            "x-queue-type".into(),
            AMQPValue::LongString("quorum".into()),
        );
        queue_args.insert("x-delivery-limit".into(), AMQPValue::LongInt(5));
        queue_args.insert(
            "x-dead-letter-exchange".into(),
            AMQPValue::LongString("flowpay.commands.dlx".into()),
        );
        self.channel
            .queue_declare(
                queue,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                queue_args,
            )
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        let dead_queue = format!("{queue}.dlq");
        self.channel
            .queue_declare(
                &dead_queue,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        self.channel
            .queue_bind(
                &dead_queue,
                "flowpay.commands.dlx",
                "",
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        for routing_key in routing_keys {
            self.channel
                .queue_bind(
                    queue,
                    "flowpay.commands",
                    routing_key,
                    QueueBindOptions::default(),
                    FieldTable::default(),
                )
                .await
                .map_err(|error| MessagingError::Rabbit(error.to_string()))?;
        }
        let mut consumer = self
            .channel
            .basic_consume(
                queue,
                "flowpay-worker",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|error| MessagingError::Rabbit(error.to_string()))?;

        while let Some(delivery) = consumer.next().await {
            let delivery = delivery.map_err(|error| MessagingError::Rabbit(error.to_string()))?;
            let envelope: MessageEnvelope = match serde_json::from_slice(&delivery.data) {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(error=%error,"malformed command moved toward DLQ");
                    delivery
                        .nack(BasicNackOptions {
                            multiple: false,
                            requeue: false,
                        })
                        .await
                        .map_err(|e| MessagingError::Rabbit(e.to_string()))?;
                    continue;
                }
            };
            match handler(envelope).await {
                Ok(()) => delivery
                    .ack(BasicAckOptions::default())
                    .await
                    .map_err(|error| MessagingError::Rabbit(error.to_string()))?,
                Err(error) => {
                    tracing::warn!(error=%error,"command failed; RabbitMQ will redeliver up to delivery limit");
                    delivery
                        .nack(BasicNackOptions {
                            multiple: false,
                            requeue: true,
                        })
                        .await
                        .map_err(|e| MessagingError::Rabbit(e.to_string()))?;
                }
            }
        }
        Ok(())
    }
}
