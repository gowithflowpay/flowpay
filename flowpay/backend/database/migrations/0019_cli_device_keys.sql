BEGIN;
CREATE TABLE merchant_cli_keys (
  public_key text PRIMARY KEY,
  merchant_id uuid NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
  created_at timestamptz NOT NULL DEFAULT now(),
  revoked_at timestamptz
);
CREATE INDEX merchant_cli_keys_merchant_idx ON merchant_cli_keys(merchant_id);
CREATE TABLE cli_key_challenges (
  id uuid PRIMARY KEY,
  public_key text NOT NULL,
  purpose text NOT NULL CHECK (purpose IN ('REGISTER','LOGIN')),
  message text NOT NULL,
  profile jsonb,
  expires_at timestamptz NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX cli_key_challenges_expiry_idx ON cli_key_challenges(expires_at);
CREATE INDEX cli_key_challenges_key_idx ON cli_key_challenges(public_key, created_at);
COMMIT;
