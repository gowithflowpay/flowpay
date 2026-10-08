BEGIN;

CREATE TABLE IF NOT EXISTS merchant_passkeys (
    credential_id text PRIMARY KEY,
    merchant_id uuid NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    passkey jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz
);
CREATE INDEX IF NOT EXISTS merchant_passkeys_merchant_idx ON merchant_passkeys(merchant_id);

CREATE TABLE IF NOT EXISTS merchant_passkey_challenges (
    id uuid PRIMARY KEY,
    merchant_id uuid NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('REGISTER','LOGIN')),
    session_hash text,
    state jsonb NOT NULL,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS merchant_passkey_challenges_expiry_idx ON merchant_passkey_challenges(expires_at);
CREATE INDEX IF NOT EXISTS merchant_passkey_challenges_merchant_idx ON merchant_passkey_challenges(merchant_id);

COMMIT;
