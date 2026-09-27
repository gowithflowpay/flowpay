BEGIN;

-- Real merchant identity. Previously merchants were seeded rows with no way to
-- sign in, so an account needs credentials, a verified email, and a record of
-- whether onboarding finished.
ALTER TABLE merchants
  ADD COLUMN IF NOT EXISTS email text,
  ADD COLUMN IF NOT EXISTS password_hash text,
  ADD COLUMN IF NOT EXISTS email_verified_at timestamptz,
  ADD COLUMN IF NOT EXISTS onboarding_completed_at timestamptz,
  ADD COLUMN IF NOT EXISTS contact_name text;

-- Emails are case-insensitive for sign-in purposes.
CREATE UNIQUE INDEX IF NOT EXISTS merchants_email_unique_idx
  ON merchants (lower(email)) WHERE email IS NOT NULL;

-- Sessions store only a hash of the bearer token, so a database leak cannot be
-- replayed against the API.
CREATE TABLE IF NOT EXISTS merchant_sessions (
    id uuid PRIMARY KEY,
    merchant_id uuid NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    token_hash text NOT NULL UNIQUE,
    user_agent text,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
CREATE INDEX IF NOT EXISTS merchant_sessions_merchant_idx
  ON merchant_sessions (merchant_id);
CREATE INDEX IF NOT EXISTS merchant_sessions_expiry_idx
  ON merchant_sessions (expires_at);

-- Six digit email verification codes, also stored hashed, with an attempt
-- counter so a code cannot be brute forced.
CREATE TABLE IF NOT EXISTS email_verification_codes (
    id uuid PRIMARY KEY,
    email text NOT NULL,
    purpose text NOT NULL DEFAULT 'SIGNUP',
    code_hash text NOT NULL,
    attempts integer NOT NULL DEFAULT 0,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS email_verification_codes_email_idx
  ON email_verification_codes (lower(email), created_at DESC);

COMMIT;
