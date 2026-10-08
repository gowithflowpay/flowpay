BEGIN;
CREATE TABLE IF NOT EXISTS browser_passkey_ceremonies (
  id uuid PRIMARY KEY,
  kind text NOT NULL CHECK (kind IN ('SIGNUP','LOGIN')),
  state jsonb NOT NULL,
  expires_at timestamptz NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS browser_passkey_ceremonies_expiry_idx ON browser_passkey_ceremonies(expires_at);
COMMIT;
