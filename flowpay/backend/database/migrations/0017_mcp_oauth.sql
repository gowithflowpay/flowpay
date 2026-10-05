BEGIN;
CREATE TABLE IF NOT EXISTS oauth_clients (
    client_id text PRIMARY KEY,
    client_name text NOT NULL,
    redirect_uris text[] NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS oauth_connections (
    id uuid PRIMARY KEY,
    merchant_id uuid NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    client_id text NOT NULL REFERENCES oauth_clients(client_id),
    resource text NOT NULL,
    scopes text[] NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz
);
CREATE INDEX IF NOT EXISTS oauth_connections_merchant_idx ON oauth_connections(merchant_id);
CREATE TABLE IF NOT EXISTS oauth_requests (
    id uuid PRIMARY KEY,
    client_id text NOT NULL REFERENCES oauth_clients(client_id),
    redirect_uri text NOT NULL,
    resource text NOT NULL,
    scopes text[] NOT NULL,
    state text,
    code_challenge text NOT NULL,
    code_hash text UNIQUE,
    connection_id uuid REFERENCES oauth_connections(id) ON DELETE CASCADE,
    status text NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING','APPROVED','DENIED','CONSUMED')),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
CREATE TABLE IF NOT EXISTS oauth_tokens (
    token_hash text PRIMARY KEY,
    connection_id uuid NOT NULL REFERENCES oauth_connections(id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('ACCESS','REFRESH')),
    issuer text NOT NULL,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS oauth_tokens_connection_idx ON oauth_tokens(connection_id);
COMMIT;
