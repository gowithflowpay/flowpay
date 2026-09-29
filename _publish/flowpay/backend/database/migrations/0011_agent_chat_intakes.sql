CREATE TABLE IF NOT EXISTS agent_chat_intakes (
    id uuid PRIMARY KEY,
    payment_id uuid NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    claim_id uuid REFERENCES claims(id) ON DELETE SET NULL,
    session_id text NOT NULL,
    email text NOT NULL,
    gathered_data jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (payment_id, session_id)
);

CREATE INDEX IF NOT EXISTS agent_chat_intakes_claim_idx
    ON agent_chat_intakes(claim_id);
