CREATE TABLE IF NOT EXISTS outbox_messages (
    id UUID PRIMARY KEY,
    message_kind TEXT NOT NULL CHECK (message_kind IN ('DOMAIN_EVENT','COMMAND')),
    destination TEXT NOT NULL,
    routing_key TEXT,
    event_id UUID NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL DEFAULT 1 CHECK (event_version > 0),
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    correlation_id TEXT,
    causation_id UUID,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ,
    locked_at TIMESTAMPTZ,
    locked_by TEXT,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT,
    CHECK ((message_kind='COMMAND' AND routing_key IS NOT NULL) OR message_kind='DOMAIN_EVENT')
);

CREATE INDEX IF NOT EXISTS idx_outbox_unpublished
    ON outbox_messages (available_at, occurred_at)
    WHERE published_at IS NULL;

CREATE TABLE IF NOT EXISTS processed_messages (
    consumer_name TEXT NOT NULL,
    event_id UUID NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (consumer_name, event_id)
);
