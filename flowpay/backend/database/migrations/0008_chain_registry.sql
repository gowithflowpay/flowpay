BEGIN;

CREATE TABLE IF NOT EXISTS chain_registry (
    chain_key text PRIMARY KEY,
    numeric_chain_id numeric(20,0) NOT NULL CHECK (numeric_chain_id > 0),
    rpc_url text NOT NULL,
    factory_address text NOT NULL,
    proxy_creation_code_hash text NOT NULL,
    factory_runtime_code_hash text,
    required_confirmations integer NOT NULL DEFAULT 3 CHECK (required_confirmations >= 0),
    enabled boolean NOT NULL DEFAULT true,
    recovery_enabled boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS chain_registry_enabled_idx
    ON chain_registry(enabled, chain_key);

COMMIT;
