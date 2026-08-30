BEGIN;

CREATE TABLE IF NOT EXISTS provider_webhook_events (
    provider text NOT NULL,
    event_id text NOT NULL,
    payload jsonb NOT NULL,
    received_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (provider, event_id)
);

CREATE INDEX IF NOT EXISTS provider_webhook_events_received_idx
    ON provider_webhook_events(received_at DESC);

COMMIT;
