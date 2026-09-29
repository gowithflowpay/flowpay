use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageKind {
    DomainEvent,
    Command,
}

impl MessageKind {
    #[must_use]
    pub const fn as_db_str(self) -> &'static str {
        match self {
            Self::DomainEvent => "DOMAIN_EVENT",
            Self::Command => "COMMAND",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageEnvelope {
    pub event_id: Uuid,
    pub event_type: String,
    pub version: u16,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub occurred_at: OffsetDateTime,
    pub correlation_id: Option<String>,
    pub causation_id: Option<Uuid>,
    pub payload: Value,
}
