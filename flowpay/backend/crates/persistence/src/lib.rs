use flowpay_domain::{
    AddressRef, Asset, AtomicAmount, ChainKey, ClaimId, ClaimState, MerchantId, OverpaymentPolicy,
    Payment, PaymentId, PaymentState, RecoveryPlan,
};
use flowpay_messaging::{enqueue_command_tx, enqueue_domain_event_tx};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::str::FromStr;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

pub const MIGRATION_0001_INIT: &str = include_str!("../../../database/migrations/0001_init.sql");
pub const MIGRATION_0002_RUNTIME: &str =
    include_str!("../../../database/migrations/0002_runtime.sql");
pub const MIGRATION_0003_AGENT_MODEL: &str =
    include_str!("../../../database/migrations/0003_agent_model.sql");
pub const MIGRATION_0004_CHAIN_KEYS: &str =
    include_str!("../../../database/migrations/0004_chain_key_canonicalization.sql");
pub const MIGRATION_0005_MESSAGING: &str =
    include_str!("../../../database/migrations/0005_messaging.sql");
pub const MIGRATION_0007_PROVIDER_WEBHOOKS: &str =
    include_str!("../../../database/migrations/0007_provider_webhooks.sql");
pub const MIGRATION_0008_CHAIN_REGISTRY: &str =
    include_str!("../../../database/migrations/0008_chain_registry.sql");
pub const MIGRATION_0009_CHAIN_AWARE_MONITORING: &str =
    include_str!("../../../database/migrations/0009_chain_aware_monitoring.sql");
pub const MIGRATION_0010_SECURITY_HARDENING: &str =
    include_str!("../../../database/migrations/0010_security_hardening.sql");

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("not found")]
    NotFound,
    #[error("invalid persisted data: {0}")]
    Invalid(String),
    #[error("concurrent update")]
    ConcurrentUpdate,
    #[error("messaging error: {0}")]
    Messaging(#[from] flowpay_messaging::MessagingError),
}

#[derive(Clone, Debug)]
pub struct CheckoutAddressRecord {
    pub chain: ChainKey,
    pub numeric_chain_id: u64,
    pub address: String,
    pub factory_address: String,
    pub factory_runtime_code_hash: Option<String>,
    pub recovery_capable: bool,
}

#[derive(Clone)]
pub struct PgStore {
    pool: PgPool,
}

#[derive(Clone, Debug)]
pub struct ApiKeyRecord {
    pub merchant_id: MerchantId,
    pub secret_hash: String,
    pub revoked: bool,
    pub expires_at: Option<OffsetDateTime>,
    pub merchant_active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredDeposit {
    pub chain: ChainKey,
    pub tx_hash: String,
    pub log_index: Option<i64>,
    pub from_address: String,
    pub to_address: String,
    pub asset_symbol: String,
    pub token_contract: Option<String>,
    pub asset_decimals: u8,
    pub amount: AtomicAmount,
    pub classification: String,
    pub confirmation_status: String,
    pub confirmations: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredClaim {
    pub id: ClaimId,
    pub public_id: String,
    pub merchant_id: MerchantId,
    pub payment_id: PaymentId,
    pub state: ClaimState,
    pub expected_chain: ChainKey,
    pub claimed_chain: Option<ChainKey>,
    pub expected_asset: String,
    pub claimed_asset: Option<String>,
    pub transaction_hash: Option<String>,
    pub originating_wallet: Option<String>,
    pub recovery_destination: String,
    pub explanation: String,
}

impl PgStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn api_key_by_prefix(&self, prefix: &str) -> Result<ApiKeyRecord, StoreError> {
        let row=sqlx::query("SELECT k.merchant_id, k.secret_hash, k.revoked_at IS NOT NULL AS revoked, k.expires_at, m.status='ACTIVE' AS merchant_active FROM api_keys k JOIN merchants m ON m.id=k.merchant_id WHERE k.public_prefix=$1 ORDER BY k.created_at DESC LIMIT 1").bind(prefix).fetch_optional(&self.pool).await?.ok_or(StoreError::NotFound)?;
        Ok(ApiKeyRecord {
            merchant_id: MerchantId(row.try_get("merchant_id")?),
            secret_hash: row.try_get("secret_hash")?,
            revoked: row.try_get("revoked")?,
            expires_at: row.try_get("expires_at")?,
            merchant_active: row.try_get("merchant_active")?,
        })
    }

    pub async fn enqueue_merchant_webhook_event(
        &self,
        merchant_id: MerchantId,
        event_type: &str,
        aggregate_type: &str,
        aggregate_public_id: &str,
        payload: Value,
    ) -> Result<String, StoreError> {
        let mut tx = self.pool.begin().await?;
        let public_id = format!("evt_{}", Uuid::now_v7().simple());
        let event_id:Uuid=sqlx::query_scalar("INSERT INTO webhook_events(public_id,merchant_id,event_type,aggregate_type,aggregate_public_id,api_version,payload) VALUES($1,$2,$3,$4,$5,'2026-08-01',$6) RETURNING id")
            .bind(&public_id).bind(merchant_id.0).bind(event_type).bind(aggregate_type).bind(aggregate_public_id).bind(payload).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO webhook_deliveries(webhook_event_id,webhook_endpoint_id,attempt,status,scheduled_at) SELECT $1,id,1,'PENDING',now() FROM webhook_endpoints WHERE merchant_id=$2 AND enabled=true AND ($3=ANY(subscribed_events) OR cardinality(subscribed_events)=0)")
            .bind(event_id).bind(merchant_id.0).bind(event_type).execute(&mut *tx).await?;
        enqueue_command_tx(
            &mut tx,
            "webhook.deliver",
            "webhook.deliver",
            "WEBHOOK_EVENT",
            &public_id,
            json!({"event_id":&public_id,"merchant_id":merchant_id.0}),
            None,
            None,
        )
        .await?;
        tx.commit().await?;
        Ok(public_id)
    }

    pub async fn create_payment(
        &self,
        payment: &Payment,
        salt: [u8; 32],
        checkout_addresses: &[CheckoutAddressRecord],
        derivation_version: &str,
    ) -> Result<(), StoreError> {
        if checkout_addresses.is_empty() {
            return Err(StoreError::Invalid(
                "at least one checkout address is required".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO payments (id,public_id,merchant_id,merchant_reference,expected_chain,expected_numeric_chain_id,expected_asset_symbol,expected_token_contract,expected_asset_decimals,expected_amount_atomic,state,overpayment_policy,required_confirmations,expires_at) VALUES ($1,$2,$3,$4,$5,NULL,$6,$7,$8,$9::numeric,$10,$11,$12,$13)")
            .bind(payment.id.0).bind(&payment.public_id).bind(payment.merchant_id.0).bind(&payment.reference).bind(payment.expected_chain.to_string()).bind(&payment.expected_asset.symbol).bind(&payment.expected_asset.token_contract).bind(i16::from(payment.expected_asset.decimals)).bind(payment.expected_amount.to_string()).bind(state_name(payment.state)).bind(overpayment_name(&payment.overpayment_policy)).bind(i32::try_from(payment.required_confirmations).unwrap_or(i32::MAX)).bind(payment.expires_at).execute(&mut *tx).await?;
        for checkout in checkout_addresses {
            sqlx::query("INSERT INTO checkout_addresses (payment_id,address_family,chain,numeric_chain_id,address,salt,factory_address,factory_runtime_code_hash,derivation_version,recovery_capable) VALUES ($1,'EVM_CREATE3',$2,$3::numeric,$4,$5,$6,$7,$8,$9)")
                .bind(payment.id.0)
                .bind(checkout.chain.to_string())
                .bind(checkout.numeric_chain_id.to_string())
                .bind(&checkout.address)
                .bind(salt.to_vec())
                .bind(&checkout.factory_address)
                .bind(&checkout.factory_runtime_code_hash)
                .bind(derivation_version)
                .bind(checkout.recovery_capable)
                .execute(&mut *tx)
                .await?;
        }
        insert_transition(
            &mut tx,
            payment.id,
            None,
            PaymentState::Created,
            "payment_created",
            "SYSTEM",
            None,
            None,
        )
        .await?;
        insert_transition(
            &mut tx,
            payment.id,
            Some(PaymentState::Created),
            PaymentState::Waiting,
            "monitoring_registered",
            "SYSTEM",
            None,
            None,
        )
        .await?;
        sqlx::query(
            "UPDATE payments SET state='WAITING', version=version+1, updated_at=now() WHERE id=$1",
        )
        .bind(payment.id.0)
        .execute(&mut *tx)
        .await?;
        let event_id = enqueue_domain_event_tx(
            &mut tx,
            "flowpay.payments",
            "payment.created",
            "PAYMENT",
            &payment.public_id,
            json!({"payment_id": &payment.public_id, "merchant_id": payment.merchant_id.0, "status": "WAITING", "chain": payment.expected_chain.to_string(), "asset": &payment.expected_asset.symbol, "amount_atomic": payment.expected_amount.to_string(), "checkout_address": &payment.checkout_address.value}),
            None,
            None,
        ).await?;
        enqueue_command_tx(
            &mut tx,
            "payment.monitor.start",
            "payment.monitor.start",
            "PAYMENT",
            &payment.public_id,
            json!({"payment_id": &payment.public_id}),
            None,
            Some(event_id),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_payment(
        &self,
        merchant_id: MerchantId,
        public_id: &str,
    ) -> Result<Payment, StoreError> {
        let row=sqlx::query("SELECT p.*, p.expected_amount_atomic::text AS expected_amount_text, c.address FROM payments p JOIN checkout_addresses c ON c.payment_id=p.id AND c.chain=p.expected_chain WHERE p.merchant_id=$1 AND p.public_id=$2").bind(merchant_id.0).bind(public_id).fetch_optional(&self.pool).await?.ok_or(StoreError::NotFound)?;
        payment_from_row(&row)
    }

    pub async fn get_payment_by_id(&self, payment_id: PaymentId) -> Result<Payment, StoreError> {
        let row=sqlx::query("SELECT p.*, p.expected_amount_atomic::text AS expected_amount_text, c.address FROM payments p JOIN checkout_addresses c ON c.payment_id=p.id AND c.chain=p.expected_chain WHERE p.id=$1").bind(payment_id.0).fetch_optional(&self.pool).await?.ok_or(StoreError::NotFound)?;
        payment_from_row(&row)
    }

    pub async fn checkout_salt(
        &self,
        payment_id: PaymentId,
        chain: &ChainKey,
    ) -> Result<[u8; 32], StoreError> {
        let bytes: Vec<u8> = sqlx::query_scalar(
            "SELECT salt FROM checkout_addresses WHERE payment_id=$1 ORDER BY (chain=$2) DESC, created_at LIMIT 1",
        )
        .bind(payment_id.0)
        .bind(chain.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        bytes
            .try_into()
            .map_err(|_| StoreError::Invalid("checkout salt must be 32 bytes".into()))
    }

    pub async fn cancel_payment(
        &self,
        merchant_id: MerchantId,
        public_id: &str,
    ) -> Result<Payment, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT id,state,version FROM payments WHERE merchant_id=$1 AND public_id=$2 FOR UPDATE").bind(merchant_id.0).bind(public_id).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let id = PaymentId(row.try_get::<Uuid, _>("id")?);
        let state = parse_payment_state(row.try_get("state")?)?;
        state
            .transition(PaymentState::Cancelled)
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
        let version: i64 = row.try_get("version")?;
        let changed=sqlx::query("UPDATE payments SET state='CANCELLED',cancelled_at=now(),updated_at=now(),version=version+1 WHERE id=$1 AND version=$2").bind(id.0).bind(version).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(StoreError::ConcurrentUpdate);
        }
        insert_transition(
            &mut tx,
            id,
            Some(state),
            PaymentState::Cancelled,
            "merchant_cancelled",
            "MERCHANT",
            None,
            None,
        )
        .await?;
        enqueue_domain_event_tx(&mut tx,"flowpay.payments","payment.failed","PAYMENT",public_id,json!({"payment_id":public_id,"merchant_id":merchant_id.0,"status":"CANCELLED","reason":"merchant_cancelled"}),None,None).await?;
        tx.commit().await?;
        self.get_payment(merchant_id, public_id).await
    }

    pub async fn retry_failed_settlement(
        &self,
        merchant_id: MerchantId,
        public_id: &str,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT id,state,expected_asset_symbol,expected_token_contract,expected_amount_atomic::text AS expected_amount FROM payments WHERE merchant_id=$1 AND public_id=$2 FOR UPDATE")
            .bind(merchant_id.0)
            .bind(public_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let payment_id = PaymentId(row.try_get("id")?);
        let state: String = row.try_get("state")?;
        if state != "FAILED" {
            return Err(StoreError::Invalid(
                "only a failed payment can retry settlement".into(),
            ));
        }
        let latest_reason: String = sqlx::query_scalar("SELECT reason_code FROM payment_state_transitions WHERE payment_id=$1 ORDER BY id DESC LIMIT 1")
            .bind(payment_id.0)
            .fetch_one(&mut *tx)
            .await?;
        if !matches!(
            latest_reason.as_str(),
            "settlement_signer_rejected" | "settlement_verification_failed"
        ) {
            return Err(StoreError::Invalid(
                "this failure class is not eligible for settlement retry".into(),
            ));
        }
        let covered: bool = sqlx::query_scalar("SELECT COALESCE(sum(amount_atomic),0) >= $4::numeric FROM deposits WHERE payment_id=$1 AND confirmation_status='FINAL' AND classification='EXPECTED_ASSET' AND upper(asset_symbol)=upper($2) AND (($3::text IS NULL AND token_contract IS NULL) OR lower(token_contract)=lower($3))")
            .bind(payment_id.0)
            .bind(row.try_get::<String, _>("expected_asset_symbol")?)
            .bind(row.try_get::<Option<String>, _>("expected_token_contract")?)
            .bind(row.try_get::<String, _>("expected_amount")?)
            .fetch_one(&mut *tx)
            .await?;
        if !covered {
            return Err(StoreError::Invalid(
                "final expected-asset deposits no longer cover the payment".into(),
            ));
        }
        let changed = sqlx::query("UPDATE payments SET state='CONFIRMED',version=version+1,updated_at=now() WHERE id=$1 AND state='FAILED'")
            .bind(payment_id.0)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if changed != 1 {
            return Err(StoreError::ConcurrentUpdate);
        }
        insert_transition(
            &mut tx,
            payment_id,
            Some(PaymentState::Failed),
            PaymentState::Confirmed,
            "settlement_retry_requested",
            "MERCHANT",
            None,
            None,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn payment_deposits(
        &self,
        payment_id: PaymentId,
    ) -> Result<Vec<StoredDeposit>, StoreError> {
        let rows=sqlx::query("SELECT chain,tx_hash,log_index,from_address,to_address,asset_symbol,token_contract,asset_decimals,amount_atomic::text AS amount,classification,confirmation_status,confirmations FROM deposits WHERE payment_id=$1 ORDER BY created_at,id").bind(payment_id.0).fetch_all(&self.pool).await?;
        rows.iter().map(deposit_from_row).collect()
    }

    pub async fn record_verified_deposit(
        &self,
        payment_id: PaymentId,
        deposit: &StoredDeposit,
        block_number: u64,
        block_hash: &str,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let chain_tx_id:Uuid=sqlx::query_scalar("INSERT INTO chain_transactions (chain,tx_hash,from_address,to_address,block_number,block_hash,tx_status,canonical,verified_at) VALUES ($1,$2,$3,$4,$5::numeric,$6,'SUCCESS',true,now()) ON CONFLICT (chain,tx_hash) DO UPDATE SET from_address=EXCLUDED.from_address,to_address=EXCLUDED.to_address,block_number=EXCLUDED.block_number,block_hash=EXCLUDED.block_hash,tx_status='SUCCESS',canonical=true,verified_at=now() RETURNING id").bind(deposit.chain.to_string()).bind(&deposit.tx_hash).bind(&deposit.from_address).bind(&deposit.to_address).bind(block_number.to_string()).bind(block_hash).fetch_one(&mut *tx).await?;
        let inserted=sqlx::query("INSERT INTO deposits (payment_id,chain_transaction_id,chain,tx_hash,log_index,from_address,to_address,asset_symbol,token_contract,asset_decimals,amount_atomic,classification,confirmation_status,observed_block_number,observed_block_hash,confirmations) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::numeric,$12,$13,$14::numeric,$15,$16) ON CONFLICT DO NOTHING")
            .bind(payment_id.0).bind(chain_tx_id).bind(deposit.chain.to_string()).bind(&deposit.tx_hash).bind(deposit.log_index).bind(&deposit.from_address).bind(&deposit.to_address).bind(&deposit.asset_symbol).bind(&deposit.token_contract).bind(i16::from(deposit.asset_decimals)).bind(deposit.amount.to_string()).bind(&deposit.classification).bind(&deposit.confirmation_status).bind(block_number.to_string()).bind(block_hash).bind(i32::try_from(deposit.confirmations).unwrap_or(i32::MAX)).execute(&mut *tx).await?.rows_affected()==1;
        sqlx::query("UPDATE deposits SET confirmation_status=$6,confirmations=$7,observed_block_hash=$8,updated_at=now() WHERE payment_id=$1 AND chain=$2 AND tx_hash=$3 AND log_index IS NOT DISTINCT FROM $4 AND token_contract IS NOT DISTINCT FROM $5")
            .bind(payment_id.0).bind(deposit.chain.to_string()).bind(&deposit.tx_hash).bind(deposit.log_index).bind(&deposit.token_contract).bind(&deposit.confirmation_status).bind(i32::try_from(deposit.confirmations).unwrap_or(i32::MAX)).bind(block_hash).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(inserted)
    }

    pub async fn set_payment_state(
        &self,
        payment_id: PaymentId,
        to: PaymentState,
        reason: &str,
        chain: Option<&ChainKey>,
        tx_hash: Option<&str>,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT state,version FROM payments WHERE id=$1 FOR UPDATE")
            .bind(payment_id.0)
            .fetch_one(&mut *tx)
            .await?;
        let from = parse_payment_state(row.try_get("state")?)?;
        if from == to {
            return Ok(());
        }
        from.transition(to)
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
        let version: i64 = row.try_get("version")?;
        let changed=sqlx::query("UPDATE payments SET state=$2,version=version+1,updated_at=now(),completed_at=CASE WHEN $2='COMPLETED' THEN now() ELSE completed_at END WHERE id=$1 AND version=$3").bind(payment_id.0).bind(state_name(to)).bind(version).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(StoreError::ConcurrentUpdate);
        }
        insert_transition(
            &mut tx,
            payment_id,
            Some(from),
            to,
            reason,
            "SYSTEM",
            chain.map(ToString::to_string).as_deref(),
            tx_hash,
        )
        .await?;
        let meta = sqlx::query("SELECT public_id,merchant_id FROM payments WHERE id=$1")
            .bind(payment_id.0)
            .fetch_one(&mut *tx)
            .await?;
        let public_id: String = meta.try_get("public_id")?;
        let merchant_id: Uuid = meta.try_get("merchant_id")?;
        if let Some(event_type) = payment_event_type(to) {
            let event_id=enqueue_domain_event_tx(&mut tx,"flowpay.payments",event_type,"PAYMENT",&public_id,json!({"payment_id":&public_id,"merchant_id":merchant_id,"status":state_name(to),"reason":reason,"chain":chain.map(ToString::to_string),"tx_hash":tx_hash}),None,None).await?;
            if matches!(to, PaymentState::Confirmed | PaymentState::Overpaid) {
                enqueue_command_tx(
                    &mut tx,
                    "payment.settlement.execute",
                    "payment.settlement.execute",
                    "PAYMENT",
                    &public_id,
                    json!({"payment_id":&public_id}),
                    None,
                    Some(event_id),
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn create_claim(&self, claim: &StoredClaim) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO claims (id,public_id,merchant_id,payment_id,state,expected_chain,claimed_chain,expected_asset,claimed_asset,claimed_transaction_hash,claimed_originating_wallet,recovery_destination,explanation) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
            .bind(claim.id.0).bind(&claim.public_id).bind(claim.merchant_id.0).bind(claim.payment_id.0).bind(claim_state_name(claim.state)).bind(claim.expected_chain.to_string()).bind(claim.claimed_chain.as_ref().map(ToString::to_string)).bind(&claim.expected_asset).bind(&claim.claimed_asset).bind(&claim.transaction_hash).bind(&claim.originating_wallet).bind(&claim.recovery_destination).bind(&claim.explanation).execute(&mut *tx).await?;
        enqueue_domain_event_tx(&mut tx,"flowpay.claims","claim.created","CLAIM",&claim.public_id,json!({"claim_id":&claim.public_id,"payment_id":claim.payment_id.0,"merchant_id":claim.merchant_id.0,"claimed_chain":claim.claimed_chain.as_ref().map(ToString::to_string),"transaction_hash":claim.transaction_hash.as_deref()}),None,None).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_claim(
        &self,
        merchant_id: MerchantId,
        public_id: &str,
    ) -> Result<StoredClaim, StoreError> {
        let row = sqlx::query("SELECT * FROM claims WHERE merchant_id=$1 AND public_id=$2")
            .bind(merchant_id.0)
            .bind(public_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StoreError::NotFound)?;
        claim_from_row(&row)
    }

    pub async fn get_claim_by_id(&self, claim_id: ClaimId) -> Result<StoredClaim, StoreError> {
        let row = sqlx::query("SELECT * FROM claims WHERE id=$1")
            .bind(claim_id.0)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StoreError::NotFound)?;
        claim_from_row(&row)
    }

    pub async fn add_claim_evidence(
        &self,
        claim_id: ClaimId,
        evidence_type: &str,
        text: Option<&str>,
        storage_key: Option<&str>,
        sha256: Option<&str>,
        submitted_by: &str,
    ) -> Result<Uuid, StoreError> {
        let id:Uuid=sqlx::query_scalar("INSERT INTO claim_evidence (claim_id,evidence_type,storage_key,content_sha256,text_content,authoritative,submitted_by) VALUES ($1,$2,$3,$4,$5,false,$6) RETURNING id").bind(claim_id.0).bind(evidence_type).bind(storage_key).bind(sha256).bind(text).bind(submitted_by).fetch_one(&self.pool).await?;
        Ok(id)
    }

    pub async fn store_wallet_challenge(
        &self,
        claim_id: ClaimId,
        chain: &ChainKey,
        wallet: &str,
        nonce: &str,
        message: &str,
        expires_at: OffsetDateTime,
    ) -> Result<Uuid, StoreError> {
        let id:Uuid=sqlx::query_scalar("INSERT INTO claim_wallet_signatures (claim_id,chain,wallet_address,challenge_nonce,challenge_message,expires_at,verification_method,verification_result) VALUES ($1,$2,$3,$4,$5,$6,'EIP191','PENDING') RETURNING id").bind(claim_id.0).bind(chain.to_string()).bind(wallet).bind(nonce).bind(message).bind(expires_at).fetch_one(&self.pool).await?;
        Ok(id)
    }

    pub async fn mark_wallet_signature(
        &self,
        challenge_id: Uuid,
        signature: &str,
        verified: bool,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE claim_wallet_signatures SET signature=$2,verified_at=CASE WHEN $3 THEN now() ELSE NULL END,verification_result=CASE WHEN $3 THEN 'VERIFIED' ELSE 'FAILED' END WHERE id=$1").bind(challenge_id).bind(signature).bind(verified).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn wallet_authorization(
        &self,
        claim_id: ClaimId,
    ) -> Result<Option<String>, StoreError> {
        Ok(sqlx::query_scalar("SELECT wallet_address FROM claim_wallet_signatures WHERE claim_id=$1 AND verification_result='VERIFIED' ORDER BY verified_at DESC LIMIT 1").bind(claim_id.0).fetch_optional(&self.pool).await?)
    }

    pub async fn list_monitorable_payment_ids(
        &self,
        chain: &ChainKey,
    ) -> Result<Vec<PaymentId>, StoreError> {
        let rows=sqlx::query_scalar::<_,Uuid>("SELECT p.id FROM payments p WHERE p.state IN ('WAITING','DETECTED','CONFIRMING','PARTIALLY_PAID','OVERPAID','WRONG_ASSET','CONFIRMED','SETTLING') AND EXISTS (SELECT 1 FROM checkout_addresses c WHERE c.payment_id=p.id AND c.chain=$1) ORDER BY p.created_at").bind(chain.to_string()).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(PaymentId).collect())
    }

    pub async fn merchant_settlement_address(
        &self,
        merchant_id: MerchantId,
    ) -> Result<String, StoreError> {
        sqlx::query_scalar("SELECT evm_settlement_address FROM merchants WHERE id=$1")
            .bind(merchant_id.0)
            .fetch_optional(&self.pool)
            .await?
            .flatten()
            .ok_or(StoreError::NotFound)
    }

    pub async fn checkout_address_for_chain(
        &self,
        payment_id: PaymentId,
        chain: &ChainKey,
    ) -> Result<String, StoreError> {
        sqlx::query_scalar(
            "SELECT address FROM checkout_addresses WHERE payment_id=$1 AND chain=$2",
        )
        .bind(payment_id.0)
        .bind(chain.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)
    }

    pub async fn monitor_cursor(
        &self,
        payment_id: PaymentId,
        chain: &ChainKey,
    ) -> Result<Option<u64>, StoreError> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT last_scanned_block::text FROM payment_monitor_cursors WHERE payment_id=$1 AND chain=$2",
        )
        .bind(payment_id.0)
        .bind(chain.to_string())
        .fetch_optional(&self.pool)
        .await?;
        value
            .map(|v| {
                v.parse::<u64>()
                    .map_err(|e| StoreError::Invalid(e.to_string()))
            })
            .transpose()
    }

    pub async fn set_monitor_cursor(
        &self,
        payment_id: PaymentId,
        chain: &ChainKey,
        height: u64,
        block_hash: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO payment_monitor_cursors(payment_id,chain,last_scanned_block,last_scanned_block_hash) VALUES($1,$2,$3::numeric,$4) ON CONFLICT(payment_id,chain) DO UPDATE SET last_scanned_block=EXCLUDED.last_scanned_block,last_scanned_block_hash=EXCLUDED.last_scanned_block_hash,updated_at=now()")
            .bind(payment_id.0).bind(chain.to_string()).bind(height.to_string()).bind(block_hash).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn asset_by_symbol(
        &self,
        chain: &ChainKey,
        symbol: &str,
    ) -> Result<Asset, StoreError> {
        let row=sqlx::query("SELECT symbol,token_contract,decimals FROM chain_assets WHERE chain=$1 AND upper(symbol)=upper($2) AND enabled=true AND purpose IN ('PAYMENT','BOTH') ORDER BY created_at LIMIT 1").bind(chain.to_string()).bind(symbol).fetch_optional(&self.pool).await?.ok_or(StoreError::NotFound)?;
        Ok(Asset {
            symbol: row.try_get("symbol")?,
            token_contract: row.try_get("token_contract")?,
            decimals: u8::try_from(row.try_get::<i16, _>("decimals")?)
                .map_err(|_| StoreError::Invalid("bad decimals".into()))?,
        })
    }

    pub async fn recovery_asset_by_contract(
        &self,
        chain: &ChainKey,
        contract: &str,
    ) -> Result<Asset, StoreError> {
        let row=sqlx::query("SELECT symbol,token_contract,decimals FROM chain_assets WHERE chain=$1 AND lower(token_contract)=lower($2) AND enabled=true AND purpose IN ('RECOVERY','BOTH') LIMIT 1").bind(chain.to_string()).bind(contract).fetch_optional(&self.pool).await?.ok_or(StoreError::NotFound)?;
        Ok(Asset {
            symbol: row.try_get("symbol")?,
            token_contract: row.try_get("token_contract")?,
            decimals: u8::try_from(row.try_get::<i16, _>("decimals")?)
                .map_err(|_| StoreError::Invalid("bad decimals".into()))?,
        })
    }

    pub async fn latest_wallet_challenge(
        &self,
        claim_id: ClaimId,
    ) -> Result<(Uuid, String, String, String, OffsetDateTime), StoreError> {
        let row=sqlx::query("SELECT id,wallet_address,challenge_nonce,challenge_message,expires_at FROM claim_wallet_signatures WHERE claim_id=$1 AND verification_result='PENDING' ORDER BY created_at DESC LIMIT 1").bind(claim_id.0).fetch_optional(&self.pool).await?.ok_or(StoreError::NotFound)?;
        Ok((
            row.try_get("id")?,
            row.try_get("wallet_address")?,
            row.try_get("challenge_nonce")?,
            row.try_get("challenge_message")?,
            row.try_get("expires_at")?,
        ))
    }

    pub async fn set_claim_state(
        &self,
        claim_id: ClaimId,
        to: ClaimState,
        reason: &str,
        actor: &str,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT state,version FROM claims WHERE id=$1 FOR UPDATE")
            .bind(claim_id.0)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let from = parse_claim_state(row.try_get("state")?)?;
        if from == to {
            return Ok(());
        }
        from.transition(to)
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
        let version: i64 = row.try_get("version")?;
        let changed=sqlx::query("UPDATE claims SET state=$2,version=version+1,updated_at=now(),closed_at=CASE WHEN $2 IN ('RECOVERED','NOT_RECOVERABLE','ESCALATED','REJECTED') THEN now() ELSE closed_at END WHERE id=$1 AND version=$3").bind(claim_id.0).bind(claim_state_name(to)).bind(version).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(StoreError::ConcurrentUpdate);
        }
        sqlx::query("INSERT INTO claim_state_transitions(claim_id,from_state,to_state,reason_code,actor_type) VALUES($1,$2,$3,$4,$5)").bind(claim_id.0).bind(claim_state_name(from)).bind(claim_state_name(to)).bind(reason).bind(actor).execute(&mut *tx).await?;
        let meta = sqlx::query("SELECT public_id,merchant_id,payment_id FROM claims WHERE id=$1")
            .bind(claim_id.0)
            .fetch_one(&mut *tx)
            .await?;
        let public_id: String = meta.try_get("public_id")?;
        let merchant_id: Uuid = meta.try_get("merchant_id")?;
        let payment_id: Uuid = meta.try_get("payment_id")?;
        if let Some(event_type) = claim_event_type(to) {
            let event_id=enqueue_domain_event_tx(&mut tx,"flowpay.claims",event_type,"CLAIM",&public_id,json!({"claim_id":&public_id,"merchant_id":merchant_id,"payment_id":payment_id,"status":claim_state_name(to),"reason":reason,"actor":actor}),None,None).await?;
            if to == ClaimState::Investigating {
                enqueue_command_tx(
                    &mut tx,
                    "claim.investigate",
                    "claim.investigate",
                    "CLAIM",
                    &public_id,
                    json!({"claim_id":&public_id}),
                    None,
                    Some(event_id),
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn insert_recovery_plan(
        &self,
        hashed: &flowpay_recovery_types::StoredPlanCompat,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO recovery_plans (id,public_id,claim_id,payment_id,source_chain,asset_symbol,token_contract,amount_atomic,checkout_address,recovery_destination,receiver_deployment_required,estimated_gas_atomic,policy_version,policy_decision,simulation_status,risk_flags,required_approval,canonical_plan,plan_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8::numeric,$9,$10,$11,$12::numeric,$13,$14,$15,$16,$17,$18,$19)")
            .bind(hashed.plan.id.0).bind(format!("rpl_{}",hashed.plan.id.0.simple())).bind(hashed.plan.claim_id.0).bind(hashed.plan.payment_id.0).bind(hashed.plan.source_chain.to_string()).bind(&hashed.plan.asset_symbol).bind(&hashed.plan.token_contract).bind(hashed.plan.amount.to_string()).bind(&hashed.plan.checkout_address.value).bind(&hashed.plan.recovery_destination.value).bind(hashed.plan.receiver_deployment_required).bind(hashed.plan.estimated_gas_atomic.to_string()).bind(&hashed.plan.policy_version).bind(format!("{:?}",hashed.plan.policy_decision).to_ascii_uppercase()).bind(format!("{:?}",hashed.plan.simulation_status).to_ascii_uppercase()).bind(hashed.plan.risk_flags.iter().map(|v|format!("{v:?}").to_ascii_uppercase()).collect::<Vec<_>>()).bind(hashed.plan.required_approval).bind(&hashed.canonical_json).bind(&hashed.plan_hash).execute(&self.pool).await?;
        Ok(())
    }
}

// Keeps persistence independent from the recovery crate and avoids a circular dependency.
pub mod flowpay_recovery_types {
    use super::*;
    #[derive(Clone, Debug)]
    pub struct StoredPlanCompat {
        pub plan: RecoveryPlan,
        pub canonical_json: Value,
        pub plan_hash: String,
    }
}

async fn insert_transition(
    tx: &mut Transaction<'_, Postgres>,
    payment_id: PaymentId,
    from: Option<PaymentState>,
    to: PaymentState,
    reason: &str,
    actor: &str,
    chain: Option<&str>,
    tx_hash: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO payment_state_transitions (payment_id,from_state,to_state,reason_code,actor_type,chain,tx_hash) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(payment_id.0).bind(from.map(state_name)).bind(state_name(to)).bind(reason).bind(actor).bind(chain).bind(tx_hash).execute(&mut **tx).await?;
    Ok(())
}

fn payment_event_type(state: PaymentState) -> Option<&'static str> {
    match state {
        PaymentState::Detected => Some("payment.detected"),
        PaymentState::PartiallyPaid => Some("payment.partially_paid"),
        PaymentState::Confirmed | PaymentState::Overpaid => Some("payment.confirmed"),
        PaymentState::Completed => Some("payment.completed"),
        PaymentState::Failed => Some("payment.failed"),
        _ => None,
    }
}
fn claim_event_type(state: ClaimState) -> Option<&'static str> {
    match state {
        ClaimState::Investigating => Some("claim.investigation.started"),
        ClaimState::Recoverable => Some("claim.recoverable"),
        ClaimState::ApprovalPending => Some("claim.approval_pending"),
        ClaimState::RecoveryPending => Some("claim.recovery_pending"),
        ClaimState::Recovered => Some("claim.recovered"),
        ClaimState::Rejected => Some("claim.rejected"),
        ClaimState::Escalated => Some("claim.escalated"),
        ClaimState::NotRecoverable => Some("claim.not_recoverable"),
        _ => None,
    }
}

fn payment_from_row(row: &sqlx::postgres::PgRow) -> Result<Payment, StoreError> {
    let chain = ChainKey::from_str(row.try_get::<&str, _>("expected_chain")?)
        .map_err(|e| StoreError::Invalid(e.to_string()))?;
    let state = parse_payment_state(row.try_get("state")?)?;
    let amount = AtomicAmount::from_str(row.try_get::<&str, _>("expected_amount_text")?)
        .map_err(|e| StoreError::Invalid(e.to_string()))?;
    Ok(Payment {
        id: PaymentId(row.try_get("id")?),
        public_id: row.try_get("public_id")?,
        merchant_id: MerchantId(row.try_get("merchant_id")?),
        reference: row.try_get("merchant_reference")?,
        expected_chain: chain.clone(),
        expected_asset: Asset {
            symbol: row.try_get("expected_asset_symbol")?,
            decimals: u8::try_from(row.try_get::<i16, _>("expected_asset_decimals")?)
                .map_err(|_| StoreError::Invalid("bad decimals".into()))?,
            token_contract: row.try_get("expected_token_contract")?,
        },
        expected_amount: amount,
        checkout_address: AddressRef {
            chain,
            value: row.try_get("address")?,
        },
        state,
        required_confirmations: u64::try_from(row.try_get::<i32, _>("required_confirmations")?)
            .map_err(|_| StoreError::Invalid("bad confirmations".into()))?,
        overpayment_policy: parse_overpayment(row.try_get("overpayment_policy")?)?,
        expires_at: row.try_get("expires_at")?,
    })
}
fn deposit_from_row(row: &sqlx::postgres::PgRow) -> Result<StoredDeposit, StoreError> {
    Ok(StoredDeposit {
        chain: ChainKey::from_str(row.try_get::<&str, _>("chain")?)
            .map_err(|e| StoreError::Invalid(e.to_string()))?,
        tx_hash: row.try_get("tx_hash")?,
        log_index: row.try_get("log_index")?,
        from_address: row.try_get("from_address")?,
        to_address: row.try_get("to_address")?,
        asset_symbol: row.try_get("asset_symbol")?,
        token_contract: row.try_get("token_contract")?,
        asset_decimals: u8::try_from(row.try_get::<i16, _>("asset_decimals")?)
            .map_err(|_| StoreError::Invalid("bad decimals".into()))?,
        amount: AtomicAmount::from_str(row.try_get::<&str, _>("amount")?)
            .map_err(|e| StoreError::Invalid(e.to_string()))?,
        classification: row.try_get("classification")?,
        confirmation_status: row.try_get("confirmation_status")?,
        confirmations: u64::try_from(row.try_get::<i32, _>("confirmations")?)
            .map_err(|_| StoreError::Invalid("bad confirmations".into()))?,
    })
}
fn claim_from_row(row: &sqlx::postgres::PgRow) -> Result<StoredClaim, StoreError> {
    Ok(StoredClaim {
        id: ClaimId(row.try_get("id")?),
        public_id: row.try_get("public_id")?,
        merchant_id: MerchantId(row.try_get("merchant_id")?),
        payment_id: PaymentId(row.try_get("payment_id")?),
        state: parse_claim_state(row.try_get("state")?)?,
        expected_chain: ChainKey::from_str(row.try_get::<&str, _>("expected_chain")?)
            .map_err(|e| StoreError::Invalid(e.to_string()))?,
        claimed_chain: row
            .try_get::<Option<String>, _>("claimed_chain")?
            .map(|s| ChainKey::from_str(&s).map_err(|e| StoreError::Invalid(e.to_string())))
            .transpose()?,
        expected_asset: row.try_get("expected_asset")?,
        claimed_asset: row.try_get("claimed_asset")?,
        transaction_hash: row.try_get("claimed_transaction_hash")?,
        originating_wallet: row.try_get("claimed_originating_wallet")?,
        recovery_destination: row.try_get("recovery_destination")?,
        explanation: row.try_get("explanation")?,
    })
}
fn state_name(s: PaymentState) -> &'static str {
    s.as_str()
}
fn parse_payment_state(s: &str) -> Result<PaymentState, StoreError> {
    serde_json::from_str(&format!("\"{s}\"")).map_err(|e| StoreError::Invalid(e.to_string()))
}
fn claim_state_name(s: ClaimState) -> &'static str {
    s.as_str()
}
fn parse_claim_state(s: &str) -> Result<ClaimState, StoreError> {
    serde_json::from_str(&format!("\"{s}\"")).map_err(|e| StoreError::Invalid(e.to_string()))
}
fn overpayment_name(v: &OverpaymentPolicy) -> &'static str {
    match v {
        OverpaymentPolicy::AcceptAndRecord => "ACCEPT_AND_RECORD",
        OverpaymentPolicy::RequireReview => "REQUIRE_REVIEW",
        OverpaymentPolicy::RejectSettlement => "REJECT_SETTLEMENT",
    }
}
fn parse_overpayment(s: &str) -> Result<OverpaymentPolicy, StoreError> {
    match s {
        "ACCEPT_AND_RECORD" => Ok(OverpaymentPolicy::AcceptAndRecord),
        "REQUIRE_REVIEW" => Ok(OverpaymentPolicy::RequireReview),
        "REJECT_SETTLEMENT" => Ok(OverpaymentPolicy::RejectSettlement),
        _ => Err(StoreError::Invalid(format!("bad overpayment policy {s}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_contains_core_audit_tables() {
        for table in [
            "payments",
            "payment_state_transitions",
            "claims",
            "recovery_plans",
            "agent_tool_calls",
            "approvals",
            "audit_logs",
        ] {
            assert!(MIGRATION_0001_INIT.contains(table), "missing table {table}");
        }
    }
}
