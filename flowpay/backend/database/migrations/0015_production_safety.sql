BEGIN;

ALTER TABLE api_keys
  ADD COLUMN IF NOT EXISTS scopes text[] NOT NULL DEFAULT ARRAY[
    'payments:read',
    'payments:write',
    'claims:read',
    'claims:write',
    'webhooks:read',
    'webhooks:write'
  ]::text[];

ALTER TABLE idempotency_keys
  ADD COLUMN IF NOT EXISTS owner_token uuid;

CREATE TABLE IF NOT EXISTS chain_execution_intents (
    id uuid PRIMARY KEY,
    operation_type text NOT NULL,
    operation_id uuid NOT NULL,
    chain text NOT NULL,
    signer_address text NOT NULL,
    nonce numeric(78,0),
    raw_transaction text,
    tx_hash text,
    state text NOT NULL,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (operation_type, operation_id),
    UNIQUE (chain, signer_address, nonce)
);
CREATE INDEX IF NOT EXISTS chain_execution_intents_reconcile_idx
  ON chain_execution_intents(state, updated_at);

COMMIT;
