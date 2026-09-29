BEGIN;

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE merchants (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    public_id text NOT NULL UNIQUE,
    name text NOT NULL,
    status text NOT NULL CHECK (status IN ('ACTIVE','DISABLED')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE api_keys (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id uuid NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    public_prefix text NOT NULL,
    secret_hash text NOT NULL,
    label text NOT NULL,
    last_used_at timestamptz,
    expires_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (merchant_id, public_prefix)
);
CREATE INDEX api_keys_active_lookup_idx ON api_keys(public_prefix) WHERE revoked_at IS NULL;

CREATE TABLE idempotency_keys (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id uuid NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    api_scope text NOT NULL,
    idempotency_key text NOT NULL,
    request_hash text NOT NULL,
    response_status integer,
    response_body jsonb,
    resource_type text,
    resource_public_id text,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (merchant_id, api_scope, idempotency_key)
);
CREATE INDEX idempotency_expiry_idx ON idempotency_keys(expires_at);

CREATE TABLE payments (
    id uuid PRIMARY KEY,
    public_id text NOT NULL UNIQUE,
    merchant_id uuid NOT NULL REFERENCES merchants(id),
    merchant_reference text,
    expected_chain text NOT NULL,
    expected_numeric_chain_id numeric(20,0),
    expected_asset_symbol text NOT NULL,
    expected_token_contract text,
    expected_asset_decimals smallint NOT NULL CHECK (expected_asset_decimals BETWEEN 0 AND 255),
    expected_amount_atomic numeric(78,0) NOT NULL CHECK (expected_amount_atomic >= 0),
    state text NOT NULL,
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    overpayment_policy text NOT NULL DEFAULT 'ACCEPT_AND_RECORD'
      CHECK (overpayment_policy IN ('ACCEPT_AND_RECORD','REQUIRE_REVIEW','REJECT_SETTLEMENT')),
    required_confirmations integer NOT NULL CHECK (required_confirmations >= 0),
    expires_at timestamptz NOT NULL,
    cancelled_at timestamptz,
    completed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX payments_merchant_created_idx ON payments(merchant_id, created_at DESC);
CREATE INDEX payments_state_idx ON payments(state, created_at);
CREATE INDEX payments_reference_idx ON payments(merchant_id, merchant_reference) WHERE merchant_reference IS NOT NULL;

CREATE TABLE checkout_addresses (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id uuid NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    address_family text NOT NULL CHECK (address_family IN ('EVM_CREATE3','SOLANA_NATIVE')),
    chain text NOT NULL,
    numeric_chain_id numeric(20,0),
    address text NOT NULL,
    salt bytea,
    factory_address text,
    factory_runtime_code_hash text,
    derivation_version text NOT NULL,
    recovery_capable boolean NOT NULL DEFAULT false,
    verified_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (payment_id, chain),
    UNIQUE (chain, address)
);
CREATE INDEX checkout_address_lookup_idx ON checkout_addresses(chain, address);

CREATE TABLE chain_transactions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    chain text NOT NULL,
    numeric_chain_id numeric(20,0),
    tx_hash text NOT NULL,
    from_address text,
    to_address text,
    native_value_atomic numeric(78,0) CHECK (native_value_atomic IS NULL OR native_value_atomic >= 0),
    block_number numeric(30,0),
    block_hash text,
    tx_status text CHECK (tx_status IN ('PENDING','SUCCESS','REVERTED','UNKNOWN')),
    canonical boolean,
    raw_verified jsonb,
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    verified_at timestamptz,
    UNIQUE (chain, tx_hash)
);
CREATE INDEX chain_transactions_block_idx ON chain_transactions(chain, block_number);

CREATE TABLE deposits (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id uuid NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    chain_transaction_id uuid NOT NULL REFERENCES chain_transactions(id),
    chain text NOT NULL,
    tx_hash text NOT NULL,
    log_index bigint,
    from_address text NOT NULL,
    to_address text NOT NULL,
    asset_symbol text NOT NULL,
    token_contract text,
    asset_decimals smallint NOT NULL CHECK (asset_decimals BETWEEN 0 AND 255),
    amount_atomic numeric(78,0) NOT NULL CHECK (amount_atomic >= 0),
    classification text NOT NULL CHECK (classification IN ('EXPECTED_ASSET','WRONG_ASSET','NATIVE_TRANSFER','UNKNOWN')),
    confirmation_status text NOT NULL CHECK (confirmation_status IN ('DETECTED','CONFIRMING','FINAL','ORPHANED')),
    observed_block_number numeric(30,0) NOT NULL,
    observed_block_hash text NOT NULL,
    confirmations integer NOT NULL DEFAULT 0 CHECK (confirmations >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX deposits_event_unique_idx
  ON deposits(chain, tx_hash, COALESCE(log_index, -1), to_address, COALESCE(token_contract, 'NATIVE'));
CREATE INDEX deposits_payment_idx ON deposits(payment_id, created_at);

CREATE TABLE payment_state_transitions (
    id bigserial PRIMARY KEY,
    payment_id uuid NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    from_state text,
    to_state text NOT NULL,
    reason_code text NOT NULL,
    actor_type text NOT NULL CHECK (actor_type IN ('SYSTEM','MERCHANT','CUSTOMER','AGENT','OPERATOR')),
    actor_id text,
    request_id text,
    claim_id uuid,
    chain text,
    tx_hash text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX payment_transitions_timeline_idx ON payment_state_transitions(payment_id, id);

CREATE TABLE settlements (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id uuid NOT NULL REFERENCES payments(id),
    chain text NOT NULL,
    asset_symbol text NOT NULL,
    token_contract text,
    amount_atomic numeric(78,0) NOT NULL CHECK (amount_atomic >= 0),
    destination text NOT NULL,
    state text NOT NULL CHECK (state IN ('CREATED','SIMULATED','SUBMITTED','CONFIRMED','FAILED')),
    tx_hash text,
    simulation_result jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (payment_id)
);

CREATE TABLE webhook_endpoints (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id uuid NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    url text NOT NULL,
    signing_secret_ciphertext bytea NOT NULL,
    enabled boolean NOT NULL DEFAULT true,
    subscribed_events text[] NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX webhook_endpoints_merchant_idx ON webhook_endpoints(merchant_id) WHERE enabled = true;

CREATE TABLE webhook_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    public_id text NOT NULL UNIQUE,
    merchant_id uuid NOT NULL REFERENCES merchants(id),
    event_type text NOT NULL,
    aggregate_type text NOT NULL CHECK (aggregate_type IN ('PAYMENT','CLAIM')),
    aggregate_public_id text NOT NULL,
    api_version text NOT NULL,
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX webhook_events_aggregate_idx ON webhook_events(aggregate_type, aggregate_public_id, created_at);

CREATE TABLE webhook_deliveries (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_event_id uuid NOT NULL REFERENCES webhook_events(id) ON DELETE CASCADE,
    webhook_endpoint_id uuid NOT NULL REFERENCES webhook_endpoints(id) ON DELETE CASCADE,
    attempt integer NOT NULL CHECK (attempt > 0),
    status text NOT NULL CHECK (status IN ('PENDING','DELIVERED','RETRY','DEAD')),
    scheduled_at timestamptz NOT NULL,
    attempted_at timestamptz,
    response_status integer,
    response_body_excerpt text,
    error_code text,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (webhook_event_id, webhook_endpoint_id, attempt)
);
CREATE INDEX webhook_delivery_queue_idx ON webhook_deliveries(status, scheduled_at);

CREATE TABLE claims (
    id uuid PRIMARY KEY,
    public_id text NOT NULL UNIQUE,
    merchant_id uuid NOT NULL REFERENCES merchants(id),
    payment_id uuid NOT NULL REFERENCES payments(id),
    state text NOT NULL,
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    expected_chain text NOT NULL,
    claimed_chain text,
    expected_asset text NOT NULL,
    claimed_asset text,
    claimed_transaction_hash text,
    claimed_originating_wallet text,
    recovery_destination text NOT NULL,
    explanation text NOT NULL,
    duplicate_of_claim_id uuid REFERENCES claims(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    closed_at timestamptz
);
CREATE INDEX claims_payment_idx ON claims(payment_id, created_at);
CREATE INDEX claims_state_idx ON claims(state, created_at);
CREATE UNIQUE INDEX claims_active_tx_dedupe_idx
  ON claims(payment_id, claimed_chain, claimed_transaction_hash)
  WHERE duplicate_of_claim_id IS NULL AND claimed_transaction_hash IS NOT NULL;

ALTER TABLE payment_state_transitions
  ADD CONSTRAINT payment_transition_claim_fk FOREIGN KEY (claim_id) REFERENCES claims(id);

CREATE TABLE claim_evidence (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    claim_id uuid NOT NULL REFERENCES claims(id) ON DELETE CASCADE,
    evidence_type text NOT NULL CHECK (evidence_type IN ('TX_HASH','SCREENSHOT','RECEIPT','EXCHANGE_WITHDRAWAL','WALLET_ADDRESS','DOCUMENT','TEXT')),
    storage_key text,
    content_sha256 text,
    text_content text,
    authoritative boolean NOT NULL DEFAULT false CHECK (authoritative = false),
    submitted_by text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX claim_evidence_claim_idx ON claim_evidence(claim_id, created_at);

CREATE TABLE claim_wallet_signatures (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    claim_id uuid NOT NULL REFERENCES claims(id) ON DELETE CASCADE,
    chain text NOT NULL,
    wallet_address text NOT NULL,
    challenge_nonce text NOT NULL UNIQUE,
    challenge_message text NOT NULL,
    signature text,
    expires_at timestamptz NOT NULL,
    verified_at timestamptz,
    verification_method text NOT NULL,
    verification_result text CHECK (verification_result IN ('PENDING','VERIFIED','FAILED','EXPIRED')),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX claim_wallet_signatures_claim_idx ON claim_wallet_signatures(claim_id, created_at);

CREATE TABLE recovery_plans (
    id uuid PRIMARY KEY,
    public_id text NOT NULL UNIQUE,
    claim_id uuid NOT NULL REFERENCES claims(id),
    payment_id uuid NOT NULL REFERENCES payments(id),
    source_chain text NOT NULL,
    source_numeric_chain_id numeric(20,0),
    asset_symbol text NOT NULL,
    token_contract text,
    amount_atomic numeric(78,0) NOT NULL CHECK (amount_atomic >= 0),
    checkout_address text NOT NULL,
    recovery_destination text NOT NULL,
    receiver_deployment_required boolean NOT NULL,
    estimated_gas_atomic numeric(78,0) NOT NULL CHECK (estimated_gas_atomic >= 0),
    policy_version text NOT NULL,
    policy_decision text NOT NULL CHECK (policy_decision IN ('ALLOWED','DENIED','REQUIRES_ESCALATION','NEEDS_FUNDING')),
    simulation_status text NOT NULL CHECK (simulation_status IN ('NOT_RUN','SUCCEEDED','FAILED')),
    simulation_result jsonb,
    risk_flags text[] NOT NULL DEFAULT '{}',
    required_approval boolean NOT NULL,
    canonical_plan jsonb NOT NULL,
    plan_hash text NOT NULL UNIQUE,
    expires_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX recovery_plans_claim_idx ON recovery_plans(claim_id, created_at DESC);

CREATE TABLE approvals (
    id uuid PRIMARY KEY,
    public_id text NOT NULL UNIQUE,
    claim_id uuid NOT NULL REFERENCES claims(id),
    recovery_plan_id uuid NOT NULL REFERENCES recovery_plans(id),
    plan_hash text NOT NULL,
    status text NOT NULL CHECK (status IN ('PENDING','APPROVED','REJECTED','EXPIRED','CONSUMED')),
    approved_by text,
    approval_nonce text NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    approved_at timestamptz,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (recovery_plan_id, plan_hash)
);
CREATE INDEX approvals_pending_idx ON approvals(status, expires_at);

CREATE TABLE recovery_executions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    recovery_plan_id uuid NOT NULL REFERENCES recovery_plans(id),
    approval_id uuid NOT NULL REFERENCES approvals(id),
    plan_hash text NOT NULL,
    transaction_class text NOT NULL,
    chain text NOT NULL,
    signer_key_ref text NOT NULL,
    tx_hash text,
    state text NOT NULL CHECK (state IN ('CREATED','SUBMITTED','CONFIRMED','FAILED')),
    receipt jsonb,
    verified_balance_delta jsonb,
    error_code text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (recovery_plan_id),
    UNIQUE (approval_id)
);

CREATE TABLE agent_runs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    public_id text NOT NULL UNIQUE,
    claim_id uuid NOT NULL REFERENCES claims(id),
    payment_id uuid NOT NULL REFERENCES payments(id),
    agent_version text NOT NULL,
    policy_version text NOT NULL,
    status text NOT NULL CHECK (status IN ('RUNNING','COMPLETED','FAILED','ESCALATED')),
    final_disposition text CHECK (final_disposition IN ('RECOVERABLE','NOT_RECOVERABLE','NEEDS_MORE_EVIDENCE','ESCALATE')),
    started_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz
);
CREATE INDEX agent_runs_claim_idx ON agent_runs(claim_id, started_at DESC);

CREATE TABLE agent_tool_calls (
    id bigserial PRIMARY KEY,
    agent_run_id uuid NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    step_number integer NOT NULL CHECK (step_number > 0),
    tool_name text NOT NULL,
    input_redacted jsonb NOT NULL,
    output_redacted jsonb,
    status text NOT NULL CHECK (status IN ('STARTED','SUCCEEDED','FAILED')),
    error_class text CHECK (error_class IN ('RETRYABLE','PERMANENT','POLICY_DENIED','AUTHORIZATION_DENIED')),
    request_id text,
    started_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    UNIQUE (agent_run_id, step_number)
);

CREATE TABLE agent_decisions (
    id bigserial PRIMARY KEY,
    agent_run_id uuid NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    step_number integer NOT NULL,
    decision_summary text NOT NULL,
    verification_summary text,
    policy_effect text,
    chosen_tool text,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (agent_run_id, step_number)
);

CREATE TABLE audit_logs (
    id bigserial PRIMARY KEY,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    request_id text,
    correlation_id text,
    merchant_id uuid REFERENCES merchants(id),
    payment_id uuid REFERENCES payments(id),
    claim_id uuid REFERENCES claims(id),
    agent_run_id uuid REFERENCES agent_runs(id),
    actor_type text NOT NULL,
    actor_id text,
    action text NOT NULL,
    chain text,
    tx_hash text,
    outcome text NOT NULL,
    metadata_redacted jsonb NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX audit_payment_idx ON audit_logs(payment_id, occurred_at);
CREATE INDEX audit_claim_idx ON audit_logs(claim_id, occurred_at);
CREATE INDEX audit_agent_idx ON audit_logs(agent_run_id, occurred_at);

COMMIT;
