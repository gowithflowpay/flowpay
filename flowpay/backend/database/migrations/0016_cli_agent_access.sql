BEGIN;

-- Headless CLI / agent bootstrap. A device (CLI process, bot, script) starts a
-- device-flow grant, the human approves it from an authenticated dashboard
-- session, and the device receives a normal merchant API key. Nothing here is
-- payable until an API key exists, so credentials remain the single authority.
CREATE TABLE IF NOT EXISTS device_grants (
    id uuid PRIMARY KEY,
    device_code_hash text NOT NULL UNIQUE,
    user_code text NOT NULL UNIQUE,
    merchant_id uuid REFERENCES merchants(id) ON DELETE SET NULL,
    api_key_id uuid,
    name text NOT NULL DEFAULT 'flowpay cli',
    scopes text[] NOT NULL DEFAULT ARRAY[
        'payments:read',
        'payments:write',
        'claims:read',
        'webhooks:read'
    ]::text[],
    state text NOT NULL DEFAULT 'PENDING',
    -- The approved API credential is stored AES-256-GCM sealed (key derived
    -- from the API-key pepper) so the plaintext never rests in the database;
    -- the poll endpoint decrypts it exactly once and consumes the grant.
    api_key_cipher bytea,
    poll_count integer NOT NULL DEFAULT 0,
    expires_at timestamptz NOT NULL,
    approved_at timestamptz,
    denied_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS device_grants_expiry_idx
  ON device_grants (expires_at);

-- Signed-challenge wallet linking. The challenge is stored hashed, consumed
-- exactly once, and expires, so a leaked response cannot be replayed.
CREATE TABLE IF NOT EXISTS wallet_challenges (
    id uuid PRIMARY KEY,
    wallet_address text NOT NULL,
    nonce_hash text NOT NULL UNIQUE,
    message text NOT NULL,
    merchant_id uuid REFERENCES merchants(id) ON DELETE CASCADE,
    session_token_hash text,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS wallet_challenges_expiry_idx
  ON wallet_challenges (expires_at);
CREATE INDEX IF NOT EXISTS wallet_challenges_address_idx
  ON wallet_challenges (lower(wallet_address));

-- One wallet can be linked to many merchants, but only once per merchant.
CREATE TABLE IF NOT EXISTS linked_wallets (
    id uuid PRIMARY KEY,
    merchant_id uuid NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    wallet_address text NOT NULL,
    label text,
    linked_at timestamptz NOT NULL DEFAULT now(),
    unlinked_at timestamptz,
    UNIQUE (merchant_id, wallet_address)
);
CREATE INDEX IF NOT EXISTS linked_wallets_address_idx
  ON linked_wallets (lower(wallet_address));

COMMIT;
