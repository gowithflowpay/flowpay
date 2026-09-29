BEGIN;

ALTER TABLE merchants
  ADD COLUMN evm_settlement_address text,
  ADD COLUMN webhook_test_mode boolean NOT NULL DEFAULT true;


CREATE TABLE claim_state_transitions (
    id bigserial PRIMARY KEY,
    claim_id uuid NOT NULL REFERENCES claims(id) ON DELETE CASCADE,
    from_state text,
    to_state text NOT NULL,
    reason_code text NOT NULL,
    actor_type text NOT NULL CHECK (actor_type IN ('SYSTEM','MERCHANT','CUSTOMER','AGENT','OPERATOR')),
    actor_id text,
    request_id text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX claim_transitions_timeline_idx ON claim_state_transitions(claim_id, id);

CREATE TABLE payment_monitor_cursors (
    payment_id uuid PRIMARY KEY REFERENCES payments(id) ON DELETE CASCADE,
    chain text NOT NULL,
    last_scanned_block numeric(30,0) NOT NULL CHECK (last_scanned_block >= 0),
    last_scanned_block_hash text,
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX payment_monitor_cursors_chain_idx ON payment_monitor_cursors(chain, last_scanned_block);

CREATE TABLE chain_assets (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    chain text NOT NULL,
    symbol text NOT NULL,
    token_contract text,
    decimals smallint NOT NULL CHECK (decimals BETWEEN 0 AND 255),
    purpose text NOT NULL CHECK (purpose IN ('PAYMENT','RECOVERY','BOTH')),
    enabled boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX chain_assets_contract_unique_idx
  ON chain_assets(chain, lower(COALESCE(token_contract, 'native')));
CREATE INDEX chain_assets_symbol_idx ON chain_assets(chain, symbol) WHERE enabled = true;

CREATE TABLE test_gas_funding (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    claim_id uuid NOT NULL REFERENCES claims(id),
    chain text NOT NULL,
    destination text NOT NULL,
    amount_atomic numeric(78,0) NOT NULL CHECK (amount_atomic > 0),
    tx_hash text,
    state text NOT NULL CHECK (state IN ('CREATED','SUBMITTED','CONFIRMED','FAILED')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE service_leases (
    lease_key text PRIMARY KEY,
    holder_id text NOT NULL,
    expires_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

COMMIT;
