use crate::{
    config::{ChainConfig, Config},
    error::ApiError,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use flowpay_chains::ChainAdapter;
use flowpay_claims::{new_wallet_challenge, verify_eip191_signature, WalletChallenge};
use flowpay_domain::{
    AddressRef, AtomicAmount, ChainKey, ClaimId, ClaimState, MerchantId, OverpaymentPolicy,
    Payment, PaymentId, PaymentState,
};
use flowpay_messaging::{enqueue_command_tx, enqueue_domain_event_tx};
use flowpay_payments::derive_checkout_salt;
use flowpay_persistence::{CheckoutAddressRecord, StoreError, StoredClaim};
use hmac::{Hmac, Mac};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::time::Duration as StdDuration;
use std::{path::PathBuf, str::FromStr};
use subtle::ConstantTimeEq;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/cli/auth/challenge", post(crate::cli_auth::challenge))
        .route("/v1/cli/auth/finish", post(crate::cli_auth::finish))
        .route("/health", get(health))
        .route("/v1/payment-assets", get(payment_assets))
        .route("/v1/providers/alchemy/webhook", post(alchemy_webhook))
        .route("/v1/payments", get(list_payments).post(create_payment))
        .route("/v1/payments/{id}", get(get_payment))
        .route("/v1/public/payments/{id}", get(public_payment))
        .route("/v1/public/payments/{id}/agent", post(public_agent_chat))
        .route("/v1/public/payments/{id}/agent/status", get(public_agent_status))
        .route(
            "/v1/public/payments/{id}/deposits",
            get(public_payment_deposits),
        )
        .route("/v1/payments/{id}/cancel", post(cancel_payment))
        .route(
            "/v1/payments/{id}/retry-settlement",
            post(retry_payment_settlement),
        )
        .route("/v1/payments/{id}/deposits", get(get_deposits))
        .route("/v1/claims", get(list_claims).post(create_claim))
        .route("/v1/claims/{id}", get(get_claim))
        .route("/v1/claims/{id}/evidence", post(add_evidence))
        .route("/v1/claims/{id}/authorize", post(authorize_claim))
        .route("/v1/claims/{id}/retry", post(retry_claim))
        .route(
            "/v1/claims/{id}/investigate",
            post(start_claim_investigation),
        )
        .route("/v1/claims/{id}/fund", post(fund_claim))
        .route("/v1/claims/{id}/approve", post(approve_claim))
        .route("/v1/webhooks", get(list_webhooks).post(create_webhook))
        .route("/v1/webhooks/test", post(test_webhook))
        .route("/v1/api-keys", get(list_api_keys).post(create_api_key))
        .route("/v1/api-keys/{id}/revoke", post(revoke_api_key))
        .route("/v1/logs", get(list_logs))
        .route("/v1/overview", get(get_overview))
        .route("/v1/merchant/overview", get(get_overview))
        .route(
            "/v1/auth/signup",
            post(crate::passkeys::legacy_auth_disabled),
        )
        .route("/v1/auth/verify", post(crate::passkeys::legacy_auth_disabled))
        .route("/v1/auth/resend", post(crate::passkeys::legacy_auth_disabled))
        .route("/v1/auth/login", post(crate::passkeys::legacy_auth_disabled))
        .route("/v1/auth/logout", post(logout))
        .route("/v1/auth/session", get(current_session))
        .route("/v1/auth/onboarding", post(complete_onboarding))
        .route("/v1/merchant/settlement-wallet", post(configure_settlement_wallet))
        .route("/v1/auth/passkeys", get(crate::passkeys::status))
        .route("/v1/auth/passkeys/signup/start", post(crate::passkeys::signup_start))
        .route("/v1/auth/passkeys/signup/finish", post(crate::passkeys::signup_finish))
        .route("/v1/auth/passkeys/discover/start", post(crate::passkeys::discover_start))
        .route("/v1/auth/passkeys/discover/finish", post(crate::passkeys::discover_finish))
        .route("/v1/auth/passkeys/register/start", post(crate::passkeys::register_start))
        .route("/v1/auth/passkeys/register/finish", post(crate::passkeys::register_finish))
        .route("/v1/auth/passkeys/login/start", post(crate::passkeys::login_start))
        .route("/v1/auth/passkeys/login/finish", post(crate::passkeys::login_finish))
        .route("/v1/agent/chat", post(agent_chat))
        .route("/v1/cli/device/start", post(crate::agent_access::device_start))
        .route("/v1/cli/device/poll", post(crate::agent_access::device_poll))
        .route("/v1/cli/device/approve", post(crate::agent_access::device_approve))
        .route("/v1/cli/device/deny", post(crate::agent_access::device_deny))
        .route("/v1/wallets/challenge", post(crate::agent_access::wallet_challenge))
        .route("/v1/wallets/verify", post(crate::agent_access::wallet_verify))
        .route("/v1/wallets", get(crate::agent_access::wallet_list))
        .route("/v1/wallets/unlink", post(crate::agent_access::wallet_unlink))
        .route("/v1/payments/{id}/events", get(crate::agent_access::payment_events))
        .route("/.well-known/oauth-authorization-server", get(crate::oauth::metadata))
        .route("/oauth/register", post(crate::oauth::register))
        .route("/oauth/authorize", get(crate::oauth::authorize))
        .route("/oauth/token", post(crate::oauth::token))
        .route("/oauth/revoke", post(crate::oauth::revoke))
        .route("/v1/oauth/requests/{id}", get(crate::oauth::request_details).post(crate::oauth::consent))
        .route("/v1/oauth/token-info", get(crate::oauth::token_info))
        .route("/v1/oauth/connections", get(crate::oauth::connections))
        .route("/v1/oauth/connections/{id}/revoke", post(crate::oauth::disconnect))
        .with_state(state)
}

async fn payment_assets(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query("SELECT DISTINCT ON (chain, upper(symbol)) chain,symbol,decimals,token_contract FROM chain_assets WHERE enabled=true AND purpose IN ('PAYMENT','BOTH') AND upper(symbol) <> 'NGN' ORDER BY chain,upper(symbol),created_at")
        .fetch_all(state.store.pool()).await.map_err(db)?;
    let mut assets = Vec::new();
    for row in rows {
        let stored_chain: String = row.try_get("chain").map_err(internal)?;
        let Ok(chain) = ChainKey::from_str(&stored_chain) else { continue };
        if chain == ChainKey::Solana || !state.chains.contains_key(&chain) { continue }
        assets.push(json!({
            "chain": stored_chain,
            "symbol": row.try_get::<String,_>("symbol").map_err(internal)?,
            "decimals": row.try_get::<i16,_>("decimals").map_err(internal)?,
            "contract": row.try_get::<Option<String>,_>("token_contract").map_err(internal)?,
        }));
    }
    Ok(Json(json!({"data": assets})))
}

async fn alchemy_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<StatusCode, ApiError> {
    let secrets = &state.config.provider_webhook_secrets;
    if secrets.is_empty() {
        return Err(ApiError::new(
            StatusCode::NOT_IMPLEMENTED,
            "provider_webhook_not_configured",
            "provider webhook secret is not configured",
        ));
    }
    let signature = headers
        .get("x-alchemy-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid_provider_signature",
                "missing Alchemy signature",
            )
        })?;
    let supplied = signature.trim_start_matches("0x");
    let valid_signature = secrets.iter().any(|secret| {
        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
            return false;
        };
        mac.update(body.as_bytes());
        let expected = hex::encode(mac.finalize().into_bytes());
        expected.len() == supplied.len()
            && expected
                .as_bytes()
                .iter()
                .zip(supplied.as_bytes())
                .fold(0_u8, |diff, (left, right)| diff | (left ^ right))
                == 0
    });
    if !valid_signature {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_provider_signature",
            "invalid Alchemy signature",
        ));
    }
    let payload: Value = serde_json::from_str(&body)
        .map_err(|_| ApiError::bad("invalid_provider_payload", "provider payload must be JSON"))?;
    let event_id = payload
        .get("id")
        .or_else(|| payload.get("webhookId"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::bad(
                "invalid_provider_payload",
                "Alchemy payload has no event identifier",
            )
        })?;
    let mut tx = state.store.pool().begin().await.map_err(db)?;
    let inserted = sqlx::query("INSERT INTO provider_webhook_events(provider,event_id,payload) VALUES('alchemy',$1,$2) ON CONFLICT DO NOTHING")
        .bind(event_id)
        .bind(&payload)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
    if inserted == 1 {
        let aggregate = payload
            .get("event")
            .and_then(|event| event.get("activity"))
            .and_then(Value::as_array)
            .and_then(|activities| activities.first())
            .and_then(|activity| activity.get("toAddress"))
            .and_then(Value::as_str)
            .unwrap_or("provider");
        enqueue_command_tx(
            &mut tx,
            "payment.reconcile",
            "payment.reconcile",
            "PROVIDER_WEBHOOK",
            aggregate,
            json!({"provider":"alchemy","event_id":event_id}),
            None,
            None,
        )
        .await
        .map_err(internal)?;
    }
    tx.commit().await.map_err(db)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn health(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    sqlx::query("SELECT 1")
        .execute(state.store.pool())
        .await
        .map_err(db)?;
    Ok(Json(json!({"ok":true,"service":"flowpay-api-server"})))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CreatePaymentRequest {
    amount: String,
    asset: String,
    chain: String,
    reference: Option<String>,
    expires_in_seconds: Option<i64>,
    overpayment_policy: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct PaymentResponse {
    id: String,
    address: String,
    amount: String,
    amount_atomic: String,
    asset: String,
    chain: String,
    status: String,
    expires_at: String,
    reference: Option<String>,
    merchant_name: Option<String>,
    checkout_url: String,
    #[serde(default)]
    payment_method: Option<String>,
    #[serde(default)]
    bank_name: Option<String>,
    #[serde(default)]
    account_name: Option<String>,
    #[serde(default)]
    account_expires_at: Option<String>,
    /// What the merchant asked for, before fee pass-through (NGN only).
    #[serde(default)]
    merchant_amount: Option<String>,
    /// Flat platform fee bundled into the total (NGN only).
    #[serde(default)]
    platform_fee: Option<String>,
    /// Flutterwave fee bundled into the total, so the customer absorbs it.
    #[serde(default)]
    estimated_provider_fee: Option<String>,
}

async fn create_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreatePaymentRequest>,
) -> Result<(StatusCode, Json<PaymentResponse>), ApiError> {
    let merchant = authenticate_scoped(&state, &headers, Some("payments:write")).await?;
    let idem = idempotency_key(&headers)?;
    let request_bytes = serde_json::to_vec(&req).map_err(internal)?;
    let request_hash = hex::encode(Sha256::digest(&request_bytes));
    if let Some(existing) =
        reserve_idempotency_key(&state, merchant, "POST:/v1/payments", &idem, &request_hash).await?
    {
        let response: PaymentResponse = serde_json::from_value(existing).map_err(internal)?;
        return Ok((StatusCode::OK, Json(response)));
    }
    if req.reference.as_ref().is_some_and(|v| v.len() > 160) {
        return Err(ApiError::bad(
            "invalid_reference",
            "reference must be <= 160 characters",
        ));
    }
    if req.asset.eq_ignore_ascii_case("NGN") || req.chain.to_ascii_lowercase().contains("flutterwave") {
        return Err(ApiError::bad("unsupported_asset", "FlowPay accepts crypto assets only"));
    }
    let chain = ChainKey::from_str(&req.chain)
        .map_err(|_| ApiError::bad("unsupported_chain", "unsupported chain"))?;
    if chain == ChainKey::Solana {
        return Err(ApiError::new(
            StatusCode::NOT_IMPLEMENTED,
            "solana_not_implemented",
            "Solana adapter is explicitly unsupported in this build",
        ));
    }
    let runtime = state
        .chains
        .get(&chain)
        .ok_or_else(|| ApiError::bad("unsupported_chain", "chain is not configured"))?;
    let asset = state
        .store
        .asset_by_symbol(&chain, &req.asset)
        .await
        .map_err(|e| match e {
            StoreError::NotFound => ApiError::bad(
                "unsupported_asset",
                "asset is not enabled for payment on this chain",
            ),
            other => db(other),
        })?;
    let amount = AtomicAmount::from_decimal(&req.amount, asset.decimals)
        .map_err(|e| ApiError::bad("invalid_amount", e.to_string()))?;
    if amount.is_zero() {
        return Err(ApiError::bad(
            "invalid_amount",
            "amount must be greater than zero",
        ));
    }
    let payment_id = PaymentId::new();
    let public_id = format!("pay_{}", payment_id.0.simple());
    let salt = derive_checkout_salt(merchant, payment_id);
    let address = runtime.deriver.checkout_hex(salt);
    let checkout_addresses: Vec<CheckoutAddressRecord> = state
        .chains
        .iter()
        .map(|(configured_chain, configured_runtime)| {
            let configured_address = configured_runtime.deriver.checkout_hex(salt);
            let configured = state
                .config
                .chains
                .get(configured_chain)
                .expect("configured runtime must have chain configuration");
            CheckoutAddressRecord {
                chain: configured_chain.clone(),
                numeric_chain_id: configured.numeric_chain_id,
                recovery_capable: configured_address.eq_ignore_ascii_case(&address)
                    && configured_runtime
                        .factory
                        .eq_ignore_ascii_case(&runtime.factory),
                address: configured_address,
                factory_address: configured_runtime.factory.clone(),
                factory_runtime_code_hash: state.config.factory_runtime_code_hash.clone(),
            }
        })
        .collect();
    if checkout_addresses
        .iter()
        .any(|entry| !entry.recovery_capable)
    {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "cross_chain_checkout_incompatible",
            "configured EVM chains do not produce one recoverable CREATE3 checkout address",
        ));
    }
    let initial_monitor_height = if !state.config.alchemy_webhook_ids.contains_key(&chain) {
        Some(runtime.adapter.health().await.map_err(|_| ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE, "chain_unavailable", "payment network is temporarily unavailable",
        ))?.latest_height.saturating_sub(3))
    } else { None };
    register_checkout_with_alchemy(&state, &address).await?;
    let expires = OffsetDateTime::now_utc()
        + Duration::seconds(req.expires_in_seconds.unwrap_or(1800).clamp(60, 2_592_000));
    let overpayment = parse_overpayment(req.overpayment_policy.as_deref())?;
    let payment = Payment {
        id: payment_id,
        public_id: public_id.clone(),
        merchant_id: merchant,
        reference: req.reference.clone(),
        expected_chain: chain.clone(),
        expected_asset: asset.clone(),
        expected_amount: amount.clone(),
        checkout_address: AddressRef {
            chain: chain.clone(),
            value: address.clone(),
        },
        state: flowpay_domain::PaymentState::Created,
        required_confirmations: if state.config.environment == "local" {
            1
        } else {
            3
        },
        overpayment_policy: overpayment,
        expires_at: expires,
    };
    state
        .store
        .create_payment(&payment, salt, &checkout_addresses, "EVM_CREATE3_V1")
        .await
        .map_err(db)?;
    if let Some(height) = initial_monitor_height {
        state.store.set_monitor_cursor(payment_id, &chain, height, None).await.map_err(db)?;
    }
    let merchant_name: String = sqlx::query_scalar("SELECT name FROM merchants WHERE id=$1")
        .bind(merchant.0)
        .fetch_one(state.store.pool())
        .await
        .map_err(db)?;
    let checkout_url = format!("{}/pay/{}", state.config.checkout_base_url, public_id);
    let response = PaymentResponse {
        id: public_id,
        address,
        amount: amount.to_decimal(asset.decimals),
        amount_atomic: amount.to_string(),
        asset: asset.symbol,
        chain: chain.to_string(),
        status: "WAITING".into(),
        expires_at: expires.to_string(),
        reference: req.reference,
        merchant_name: Some(merchant_name),
        checkout_url,
        payment_method: None,
        bank_name: None,
        account_name: None,
        account_expires_at: None,
        merchant_amount: None,
        platform_fee: None,
        estimated_provider_fee: None,
    };
    store_idempotent_response(
        &state,
        merchant,
        "POST:/v1/payments",
        &idem,
        &request_hash,
        &response,
        "PAYMENT",
        &response.id,
    )
    .await?;
    enqueue_event(
        &state,
        merchant,
        "payment.created",
        "PAYMENT",
        &response.id,
        serde_json::to_value(&response).map_err(internal)?,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn create_ngn_payment(
    state: AppState,
    merchant: MerchantId,
    idem: String,
    request_hash: String,
    req: CreatePaymentRequest,
) -> Result<(StatusCode, Json<PaymentResponse>), ApiError> {
    let secret = state
        .config
        .flutterwave_secret_key
        .as_deref()
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "flutterwave_not_configured",
                "Flutterwave is not configured",
            )
        })?;
    let email = state.config.flutterwave_customer_email.trim();
    let amount = AtomicAmount::from_decimal(&req.amount, 2)
        .map_err(|e| ApiError::bad("invalid_amount", e.to_string()))?;
    if amount.is_zero() {
        return Err(ApiError::bad(
            "invalid_amount",
            "amount must be greater than zero",
        ));
    }
    // The customer covers the platform fee and Flutterwave's cut, so the
    // merchant still nets exactly the amount they asked for.
    let platform_fee = AtomicAmount::from_biguint(BigUint::from(
        state.config.ngn_platform_fee_atomic,
    ));
    let charge_total = ngn_charge_total(
        &amount,
        &platform_fee,
        state.config.flutterwave_fee_bps,
        state.config.flutterwave_fee_vat_bps,
    );
    let provider_fee_estimate = ngn_fee_component(
        &charge_total,
        &AtomicAmount::from_biguint(amount.inner() + platform_fee.inner()),
    );
    let payment_id = PaymentId::new();
    let public_id = format!("pay_{}", payment_id.0.simple());
    let expires_in = req.expires_in_seconds.unwrap_or(3600).clamp(60, 5_270_400);
    let expires = OffsetDateTime::now_utc() + Duration::seconds(expires_in);
    let firstname = state.config.flutterwave_customer_first_name.trim();
    let lastname = state.config.flutterwave_customer_last_name.trim();
    let provider_response = state
        .http
        .post(format!(
            "{}/virtual-account-numbers",
            state.config.flutterwave_base_url
        ))
        .timeout(StdDuration::from_secs(20))
        .bearer_auth(secret)
        .json(&json!({
            "email": email,
            "tx_ref": public_id,
            "amount": serde_json::from_str::<Value>(&charge_total.to_decimal(2)).map_err(internal)?,
            "currency": "NGN",
            "firstname": firstname,
            "lastname": lastname,
            "is_permanent": false,
            "expires": expires_in.to_string(),
            "narration": "FlowPay payment"
        }))
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "flutterwave_unavailable",
                error.to_string(),
            )
        })?;
    let provider_status = provider_response.status();
    let provider_body: Value = provider_response.json().await.map_err(|_| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_flutterwave_response",
            "Flutterwave returned invalid JSON",
        )
    })?;
    if !provider_status.is_success()
        || provider_body.get("status").and_then(Value::as_str) != Some("success")
    {
        sqlx::query("DELETE FROM idempotency_keys WHERE merchant_id=$1 AND api_scope='POST:/v1/payments' AND idempotency_key=$2 AND response_body IS NULL")
            .bind(merchant.0).bind(&idem).execute(state.store.pool()).await.map_err(db)?;
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "flutterwave_account_creation_failed",
            "Flutterwave could not create the temporary account",
        ));
    }
    let data = provider_body.get("data").cloned().unwrap_or(Value::Null);
    let account_number = data
        .get("account_number")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "invalid_flutterwave_response",
                "Flutterwave returned no account number",
            )
        })?;
    let bank_name = data
        .get("bank_name")
        .and_then(Value::as_str)
        .unwrap_or("Flutterwave MFB");
    let provider_reference = data
        .get("order_ref")
        .or_else(|| data.get("flw_ref"))
        .and_then(Value::as_str)
        .unwrap_or(&public_id);
    let merchant_name: String = sqlx::query_scalar("SELECT name FROM merchants WHERE id=$1")
        .bind(merchant.0)
        .fetch_one(state.store.pool())
        .await
        .map_err(db)?;
    let checkout_url = format!("{}/pay/{}", state.config.checkout_base_url, public_id);
    let response = PaymentResponse {
        id: public_id.clone(),
        address: account_number.to_owned(),
        amount: charge_total.to_decimal(2),
        amount_atomic: charge_total.to_string(),
        asset: "NGN".into(),
        chain: "custom:flutterwave_ngn".into(),
        status: "WAITING".into(),
        expires_at: expires.to_string(),
        reference: req.reference.clone(),
        merchant_name: Some(merchant_name),
        checkout_url,
        payment_method: Some("bank_transfer".into()),
        bank_name: Some(bank_name.to_owned()),
        account_name: data
            .get("account_name")
            .and_then(Value::as_str)
            .map(str::to_owned),
        account_expires_at: data
            .get("expiry_date")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| Some(expires.to_string())),
        merchant_amount: Some(amount.to_decimal(2)),
        platform_fee: Some(platform_fee.to_decimal(2)),
        estimated_provider_fee: Some(provider_fee_estimate.to_decimal(2)),
    };
    let mut tx = state.store.pool().begin().await.map_err(db)?;
    sqlx::query("INSERT INTO payments(id,public_id,merchant_id,merchant_reference,expected_chain,expected_asset_symbol,expected_asset_decimals,expected_amount_atomic,state,overpayment_policy,required_confirmations,expires_at,merchant_amount_atomic,platform_fee_atomic) VALUES($1,$2,$3,$4,'custom:flutterwave_ngn','NGN',2,$5::numeric,'WAITING','REQUIRE_REVIEW',0,$6,$7::numeric,$8::numeric)")
        .bind(payment_id.0).bind(&public_id).bind(merchant.0).bind(&req.reference).bind(charge_total.to_string()).bind(expires).bind(amount.to_string()).bind(platform_fee.to_string())
        .execute(&mut *tx).await.map_err(db)?;
    sqlx::query("INSERT INTO checkout_addresses(payment_id,address_family,chain,address,derivation_version,recovery_capable) VALUES($1,'FIAT_VIRTUAL_ACCOUNT','custom:flutterwave_ngn',$2,'FLUTTERWAVE_DYNAMIC_V1',false)")
        .bind(payment_id.0).bind(account_number).execute(&mut *tx).await.map_err(db)?;
    sqlx::query("INSERT INTO fiat_payment_accounts(payment_id,provider,provider_reference,provider_account_id,account_number,bank_name,account_name,customer_email,expires_at,provider_payload) VALUES($1,'flutterwave',$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(payment_id.0).bind(provider_reference).bind(data.get("flw_ref").and_then(Value::as_str)).bind(account_number).bind(bank_name)
        .bind(&response.account_name).bind(email).bind(expires).bind(&provider_body).execute(&mut *tx).await.map_err(db)?;
    sqlx::query("INSERT INTO payment_state_transitions(payment_id,from_state,to_state,reason_code,actor_type,metadata) VALUES($1,NULL,'WAITING','flutterwave_account_created','SYSTEM',$2)")
        .bind(payment_id.0).bind(json!({"provider":"flutterwave","account_expires_at":response.account_expires_at})).execute(&mut *tx).await.map_err(db)?;
    enqueue_domain_event_tx(&mut tx,"flowpay.payments","payment.created","PAYMENT",&public_id,
        json!({"payment_id":public_id,"merchant_id":merchant.0,"status":"WAITING","chain":"custom:flutterwave_ngn","asset":"NGN","amount_atomic":amount.to_string(),"payment_method":"bank_transfer"}),None,None)
        .await.map_err(internal)?;
    tx.commit().await.map_err(db)?;
    store_idempotent_response(
        &state,
        merchant,
        "POST:/v1/payments",
        &idem,
        &request_hash,
        &response,
        "PAYMENT",
        &response.id,
    )
    .await?;
    enqueue_event(
        &state,
        merchant,
        "payment.created",
        "PAYMENT",
        &response.id,
        serde_json::to_value(&response).map_err(internal)?,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn flutterwave_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<StatusCode, ApiError> {
    let secret_hash = state
        .config
        .flutterwave_secret_hash
        .as_deref()
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                "flutterwave_webhook_not_configured",
                "Flutterwave webhook secret is not configured",
            )
        })?;
    let signature_ok = if let Some(signature) = headers
        .get("flutterwave-signature")
        .and_then(|v| v.to_str().ok())
    {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret_hash.as_bytes()).map_err(internal)?;
        mac.update(body.as_bytes());
        let expected = B64.encode(mac.finalize().into_bytes());
        expected.as_bytes().ct_eq(signature.as_bytes()).unwrap_u8() == 1
    } else {
        headers
            .get("verif-hash")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.as_bytes().ct_eq(secret_hash.as_bytes()).unwrap_u8() == 1)
    };
    if !signature_ok {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_flutterwave_signature",
            "invalid Flutterwave signature",
        ));
    }
    let payload: Value = serde_json::from_str(&body).map_err(|_| {
        ApiError::bad(
            "invalid_flutterwave_payload",
            "webhook payload must be JSON",
        )
    })?;
    let data = payload.get("data").unwrap_or(&payload);
    let transaction_id = value_as_string(data.get("id")).ok_or_else(|| {
        ApiError::bad("invalid_flutterwave_payload", "transaction id is required")
    })?;
    let tx_ref = data
        .get("tx_ref")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad("invalid_flutterwave_payload", "tx_ref is required"))?;
    // Acknowledge straight away. Verification and the ledger posting are two
    // outbound calls, so doing them inline made this endpoint the slowest part
    // of a payment; the worker now does that work and the reconciliation sweep
    // stays the safety net if the command is ever lost.
    let mut tx = state.store.pool().begin().await.map_err(db)?;
    enqueue_command_tx(
        &mut tx,
        "flowpay.commands",
        "flutterwave.reconcile",
        "PAYMENT",
        tx_ref,
        json!({"tx_ref": tx_ref, "transaction_id": transaction_id}),
        None,
        None,
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(db)?;
    Ok(StatusCode::ACCEPTED)
}

/// Verifies a Flutterwave transaction against the FlowPay payment and, when the
/// verified facts match, posts it to the ledger and marks the payment
/// completed. Shared by the webhook handler and the reconciliation sweep so a
/// missed webhook delivery cannot leave a paid invoice stuck in `WAITING`.
pub(crate) async fn complete_flutterwave_payment(
    state: &AppState,
    tx_ref: &str,
    transaction_id: &str,
) -> Result<bool, ApiError> {
    let row = sqlx::query("SELECT p.id,p.public_id,p.merchant_id,p.state,p.expected_amount_atomic::text AS amount_atomic,p.merchant_amount_atomic::text AS merchant_amount_atomic,p.platform_fee_atomic::text AS platform_fee_atomic,f.account_number FROM payments p JOIN fiat_payment_accounts f ON f.payment_id=p.id WHERE p.public_id=$1 AND f.provider='flutterwave'")
        .bind(tx_ref).fetch_optional(state.store.pool()).await.map_err(db)?.ok_or_else(|| ApiError::not_found())?;
    let current_state: String = row.try_get("state").map_err(internal)?;
    if current_state == "COMPLETED" {
        return Ok(false);
    }
    let secret = state
        .config
        .flutterwave_secret_key
        .as_deref()
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "flutterwave_not_configured",
                "Flutterwave is not configured",
            )
        })?;
    let verify = state
        .http
        .get(format!(
            "{}/transactions/{}/verify",
            state.config.flutterwave_base_url, transaction_id
        ))
        .timeout(StdDuration::from_secs(20))
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "flutterwave_verification_failed",
                e.to_string(),
            )
        })?;
    let verified: Value = verify.json().await.map_err(|_| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_flutterwave_response",
            "Flutterwave verification returned invalid JSON",
        )
    })?;
    let verified_data = verified.get("data").unwrap_or(&Value::Null);
    let verified_ref = verified_data
        .get("tx_ref")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let verified_status = verified_data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let verified_currency = verified_data
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let verified_amount = value_as_string(verified_data.get("amount"))
        .and_then(|v| AtomicAmount::from_decimal(&v, 2).ok())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "invalid_flutterwave_verification",
                "Flutterwave returned an invalid amount",
            )
        })?;
    let expected =
        AtomicAmount::from_str(row.try_get::<&str, _>("amount_atomic").map_err(internal)?)
            .map_err(internal)?;
    if verified_ref != tx_ref
        || !verified_status.eq_ignore_ascii_case("successful")
        || !verified_currency.eq_ignore_ascii_case("NGN")
        || verified_amount < expected
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "flutterwave_payment_mismatch",
            "verified Flutterwave payment does not match the FlowPay payment",
        ));
    }
    let payment_id: Uuid = row.try_get("id").map_err(internal)?;
    let merchant_id: Uuid = row.try_get("merchant_id").map_err(internal)?;
    // Split what was collected. Payments created before fee pass-through (and
    // any non-grossed-up row) attribute the whole amount to the merchant.
    let merchant_amount = match row
        .try_get::<Option<&str>, _>("merchant_amount_atomic")
        .map_err(internal)?
    {
        Some(value) => AtomicAmount::from_str(value).map_err(internal)?,
        None => expected.clone(),
    };
    let platform_fee = match row
        .try_get::<Option<&str>, _>("platform_fee_atomic")
        .map_err(internal)?
    {
        Some(value) => AtomicAmount::from_str(value).map_err(internal)?,
        None => AtomicAmount::zero(),
    };
    let provider_fee = ngn_fee_component(
        &verified_amount,
        &AtomicAmount::from_biguint(merchant_amount.inner() + platform_fee.inner()),
    );
    // What Flutterwave actually kept, recorded for reconciliation reporting.
    let provider_fee_actual = match (
        value_as_string(verified_data.get("charged_amount")),
        value_as_string(verified_data.get("amount_settled")),
    ) {
        (Some(charged), Some(settled)) => AtomicAmount::from_decimal(&charged, 2)
            .ok()
            .zip(AtomicAmount::from_decimal(&settled, 2).ok())
            .map(|(charged, settled)| ngn_fee_component(&charged, &settled)),
        _ => None,
    };
    let formance_reference = format!("flutterwave:{}", transaction_id);
    post_formance_ngn(
        state,
        merchant_id,
        &formance_reference,
        &merchant_amount,
        &platform_fee,
        &provider_fee,
        tx_ref,
        transaction_id,
    )
    .await?;
    let mut tx = state.store.pool().begin().await.map_err(db)?;
    sqlx::query("INSERT INTO fiat_payment_receipts(provider,provider_transaction_id,payment_id,amount_atomic,currency,provider_reference,formance_reference,verified_payload,posted_at) VALUES('flutterwave',$1,$2,$3::numeric,'NGN',$4,$5,$6,now()) ON CONFLICT(provider,provider_transaction_id) DO NOTHING")
        .bind(transaction_id).bind(payment_id).bind(verified_amount.to_string()).bind(tx_ref).bind(&formance_reference).bind(&verified).execute(&mut *tx).await.map_err(db)?;
    sqlx::query("UPDATE payments SET provider_fee_atomic=$2::numeric WHERE id=$1")
        .bind(payment_id)
        .bind(provider_fee_actual.as_ref().map(ToString::to_string))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let changed = sqlx::query("UPDATE payments SET state='COMPLETED',version=version+1,completed_at=now(),updated_at=now() WHERE id=$1 AND state NOT IN ('COMPLETED','CANCELLED','EXPIRED')")
        .bind(payment_id).execute(&mut *tx).await.map_err(db)?.rows_affected();
    if changed == 1 {
        sqlx::query("INSERT INTO payment_state_transitions(payment_id,from_state,to_state,reason_code,actor_type,chain,tx_hash,metadata) VALUES($1,$2,'COMPLETED','flutterwave_verified_and_ledgered','SYSTEM','custom:flutterwave_ngn',$3,$4)")
            .bind(payment_id).bind(&current_state).bind(transaction_id).bind(json!({"provider":"flutterwave","formance_reference":formance_reference})).execute(&mut *tx).await.map_err(db)?;
        enqueue_domain_event_tx(&mut tx,"flowpay.payments","payment.completed","PAYMENT",tx_ref,
            json!({"payment_id":tx_ref,"merchant_id":merchant_id,"status":"COMPLETED","chain":"custom:flutterwave_ngn","asset":"NGN","amount_atomic":verified_amount.to_string(),"provider_transaction_id":transaction_id}),None,None)
            .await.map_err(internal)?;
    }
    tx.commit().await.map_err(db)?;
    if changed == 1 {
        enqueue_event(state,MerchantId(merchant_id),"payment.completed","PAYMENT",tx_ref,json!({"id":tx_ref,"status":"COMPLETED","asset":"NGN","amount_atomic":verified_amount.to_string(),"provider":"flutterwave"})).await?;
    }
    Ok(changed == 1)
}

/// Looks up the Flutterwave transaction id for a FlowPay payment reference.
/// Returns `None` while Flutterwave has no successful transaction for the
/// reference yet, so the sweep only spends a verification call on paid invoices.
pub(crate) async fn find_flutterwave_transaction(
    state: &AppState,
    tx_ref: &str,
) -> Result<Option<String>, ApiError> {
    let Some(secret) = state.config.flutterwave_secret_key.as_deref() else {
        return Ok(None);
    };
    let response = state
        .http
        .get(format!("{}/transactions", state.config.flutterwave_base_url))
        .query(&[("tx_ref", tx_ref)])
        .timeout(StdDuration::from_secs(20))
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "flutterwave_lookup_failed",
                e.to_string(),
            )
        })?;
    let body: Value = response.json().await.map_err(|_| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_flutterwave_response",
            "Flutterwave transaction lookup returned invalid JSON",
        )
    })?;
    let Some(transaction) = body
        .get("data")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
    else {
        return Ok(None);
    };
    let successful = transaction
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|v| v.eq_ignore_ascii_case("successful"));
    if !successful {
        return Ok(None);
    }
    Ok(value_as_string(transaction.get("id")))
}

/// Posts a collected NGN payment to the ledger, attributing every naira: the
/// merchant's requested amount, the platform's fee, and the provider's cut.
/// The three always sum to the total the customer paid, so the ledger balances
/// against the money that actually arrived.
async fn post_formance_ngn(
    state: &AppState,
    merchant_id: Uuid,
    reference: &str,
    merchant_amount: &AtomicAmount,
    platform_fee: &AtomicAmount,
    provider_fee: &AtomicAmount,
    payment_id: &str,
    provider_id: &str,
) -> Result<(), ApiError> {
    let url = format!(
        "{}/api/ledger/v2/{}/transactions",
        state.config.formance_base_url, state.config.formance_ledger
    );
    let mut postings: Vec<Value> = Vec::new();
    for (destination, amount) in [
        (
            format!("merchants:{merchant_id}:receivable"),
            merchant_amount,
        ),
        ("platform:revenue".to_owned(), platform_fee),
        ("platform:payment-fees".to_owned(), provider_fee),
    ] {
        if amount.is_zero() {
            continue;
        }
        postings.push(json!({
            "source":"world",
            "destination":destination,
            "amount":amount.to_string().parse::<u64>().map_err(|_| {
                ApiError::bad(
                    "amount_out_of_range",
                    "NGN amount exceeds the Formance posting limit",
                )
            })?,
            "asset":"NGN/2"
        }));
    }
    if postings.is_empty() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "empty_ledger_posting",
            "collected payment has nothing to post to the ledger",
        ));
    }
    let mut request = state.http.post(url).timeout(StdDuration::from_secs(20)).header("Idempotency-Key",reference).json(&json!({
        "postings":postings,
        "reference":reference,
        "metadata":{"payment_id":payment_id,"provider":"flutterwave","provider_transaction_id":provider_id}
    }));
    if let Some(token) = state.config.formance_token.as_deref() {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|e| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "formance_unavailable",
            e.to_string(),
        )
    })?;
    if !response.status().is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "formance_posting_failed",
            format!(
                "Formance rejected the ledger posting with status {}",
                response.status()
            ),
        ));
    }
    Ok(())
}

fn value_as_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(v) => Some(v.clone()),
        Value::Number(v) => Some(v.to_string()),
        _ => None,
    }
}

/// Grosses up a merchant's requested NGN amount so the merchant still nets it
/// after the flat platform fee and Flutterwave's percentage fee (plus VAT on
/// that fee) are taken out of what the customer is charged.
///
/// Solving `total - total*fee*(1+vat) = merchant + platform` gives
/// `total = (merchant + platform) / (1 - fee*(1+vat))`, computed here in exact
/// integer arithmetic and rounded up so the merchant is never left short.
pub(crate) fn ngn_charge_total(
    merchant_atomic: &AtomicAmount,
    platform_fee_atomic: &AtomicAmount,
    fee_bps: u64,
    vat_bps: u64,
) -> AtomicAmount {
    // 1.0 expressed at 4 decimal places of both the fee and the VAT on it.
    let scale = BigUint::from(100_000_000u64);
    let deduction = BigUint::from(fee_bps) * BigUint::from(10_000u64 + vat_bps);
    let net = merchant_atomic.inner() + platform_fee_atomic.inner();
    if deduction >= scale {
        return AtomicAmount::from_biguint(net);
    }
    let denominator = scale.clone() - deduction;
    let numerator = net * scale;
    let total = (numerator + &denominator - BigUint::from(1u64)) / denominator;
    AtomicAmount::from_biguint(total)
}

/// The part of a total that is not owed to the merchant or the platform, i.e.
/// the fee the payment provider keeps.
fn ngn_fee_component(total: &AtomicAmount, net: &AtomicAmount) -> AtomicAmount {
    if total.inner() >= net.inner() {
        AtomicAmount::from_biguint(total.inner() - net.inner())
    } else {
        AtomicAmount::zero()
    }
}

async fn register_checkout_with_alchemy(
    state: &AppState,
    checkout_address: &str,
) -> Result<(), ApiError> {
    if state.config.environment.eq_ignore_ascii_case("local") {
        return Ok(());
    }
    let Some(token) = state.config.alchemy_notify_auth_token.as_deref() else {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "alchemy_notify_auth_token_missing",
            "ALCHEMY_NOTIFY_AUTH_TOKEN is required when FLOWPAY_ENV is not local",
        ));
    };

    let networks = alchemy_webhook_networks(&state.config);
    if networks.is_empty() && !state.config.alchemy_networks.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "alchemy_networks_unmatched",
            "ALCHEMY_NETWORKS does not match any configured EVM chain",
        ));
    }

    for (network, chain) in &networks {
        let webhook_id = state
            .config
            .alchemy_webhook_ids
            .get(chain)
            .ok_or_else(|| {
                let env_key = format!(
                    "ALCHEMY_{}_WEBHOOK_ID",
                    chain.to_string().to_ascii_uppercase(),
                );
                ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "alchemy_webhook_not_configured",
                    format!("webhook ID is not configured for {network} (chain {chain}); add {env_key} or pre-create the webhook on Alchemy"),
                )
            })?;

        let response = state
            .http
            .patch(&state.config.alchemy_notify_endpoint)
            .timeout(StdDuration::from_secs(10))
            .header("X-Alchemy-Token", token)
            .json(&json!({
                "webhook_id": webhook_id,
                "addresses_to_add": [checkout_address],
                "addresses_to_remove": []
            }))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(%network, webhook_id=%webhook_id, %checkout_address, "Alchemy checkout address registered");
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                tracing::error!(%status, body=%body, %network, webhook_id=%webhook_id, %checkout_address, "Alchemy checkout address registration failed");
                return Err(ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "alchemy_checkout_sync_failed",
                    format!("Alchemy rejected checkout address registration for {network} (webhook {webhook_id}, status {status})"),
                ));
            }
            Err(error) => {
                tracing::error!(%error, %network, webhook_id=%webhook_id, %checkout_address, "Alchemy checkout address registration request failed");
                return Err(ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "alchemy_checkout_sync_failed",
                    format!("Alchemy address synchronization request failed for {network} (webhook {webhook_id})"),
                ));
            }
        }
    }
    Ok(())
}

/// Webhook networks that already have a configured webhook ID.
/// Checkout creation only uses existing webhooks; it never creates one.
fn alchemy_webhook_networks(config: &Config) -> Vec<(String, ChainKey)> {
    let configured = if config.alchemy_networks.is_empty() {
        vec!["BASE_SEPOLIA".to_owned(), "ETH_SEPOLIA".to_owned()]
    } else {
        config.alchemy_networks.clone()
    };
    configured
        .into_iter()
        .filter_map(|value| {
            let network = normalize_alchemy_network(&value)?;
            let chain = alchemy_chain_for_network(&network)?;
            config
                .alchemy_webhook_ids
                .contains_key(&chain)
                .then_some((network, chain))
        })
        .collect()
}

fn normalize_alchemy_network(value: &str) -> Option<String> {
    let normalized = value.trim().replace('-', "_").to_ascii_uppercase();
    match normalized.as_str() {
        "BASE_SEPOLIA" => Some("BASE_SEPOLIA".into()),
        "ETH_SEPOLIA" | "ETHEREUM_SEPOLIA" => Some("ETH_SEPOLIA".into()),
        "ARB_SEPOLIA" | "ARBITRUM_SEPOLIA" => Some("ARB_SEPOLIA".into()),
        "OPT_SEPOLIA" | "OPTIMISM_SEPOLIA" => Some("OPT_SEPOLIA".into()),
        "MATIC_AMOY" | "POLYGON_AMOY" => Some("MATIC_AMOY".into()),
        "BNB_TESTNET" | "BSC_TESTNET" => Some("BNB_TESTNET".into()),
        _ => None,
    }
}

fn alchemy_chain_for_network(network: &str) -> Option<ChainKey> {
    Some(match network {
        "BASE_SEPOLIA" => ChainKey::Custom("base_sepolia".into()),
        "ETH_SEPOLIA" => ChainKey::Custom("ethereum_sepolia".into()),
        "ARB_SEPOLIA" => ChainKey::Custom("arbitrum_sepolia".into()),
        "OPT_SEPOLIA" => ChainKey::Custom("optimism_sepolia".into()),
        "MATIC_AMOY" => ChainKey::Custom("polygon_amoy".into()),
        "BNB_TESTNET" => ChainKey::Custom("bsc_testnet".into()),
        _ => return None,
    })
}

async fn get_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate_scoped(&state, &headers, Some("payments:read")).await?;
    let p = state
        .store
        .get_payment(merchant, &id)
        .await
        .map_err(map_store)?;
    Ok(Json(payment_detail(&state, p).await?))
}

/// The customer-facing payment view. A payer has no merchant credential, and
/// the payload below deliberately excludes every merchant-private field
/// (settlement address, provider references, internal ids).
async fn public_payment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let p = state
        .store
        .get_payment_by_public_id(&id)
        .await
        .map_err(map_store)?;
    Ok(Json(payment_detail(&state, p).await?))
}

async fn public_payment_deposits(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let p = state
        .store
        .get_payment_by_public_id(&id)
        .await
        .map_err(map_store)?;
    let deposits = state.store.payment_deposits(p.id).await.map_err(db)?;
    Ok(Json(json!({"data":deposits})))
}

/// Builds the crypto checkout and dashboard representation for one payment.
async fn payment_detail(state: &AppState, p: Payment) -> Result<Value, ApiError> {
    let name: String = sqlx::query_scalar("SELECT name FROM merchants WHERE id=$1")
        .bind(p.merchant_id.0)
        .fetch_one(state.store.pool())
        .await
        .map_err(db)?;
    Ok(payment_json(&p, &state.config.checkout_base_url, &name))
}
async fn cancel_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate_scoped(&state, &headers, Some("payments:write")).await?;
    let p = state
        .store
        .cancel_payment(merchant, &id)
        .await
        .map_err(map_store)?;
    enqueue_event(
        &state,
        merchant,
        "payment.failed",
        "PAYMENT",
        &id,
        json!({"id":id,"status":"CANCELLED","reason":"merchant_cancelled"}),
    )
    .await?;
    let name: String = sqlx::query_scalar("SELECT name FROM merchants WHERE id=$1")
        .bind(merchant.0)
        .fetch_one(state.store.pool())
        .await
        .map_err(db)?;
    Ok(Json(payment_json(
        &p,
        &state.config.checkout_base_url,
        &name,
    )))
}
async fn retry_payment_settlement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    state
        .store
        .retry_failed_settlement(merchant, &id)
        .await
        .map_err(|error| match error {
            StoreError::NotFound => ApiError::new(
                StatusCode::NOT_FOUND,
                "payment_not_found",
                "payment was not found",
            ),
            StoreError::Invalid(message) => ApiError::bad("settlement_retry_rejected", message),
            other => db(other),
        })?;
    Ok(Json(
        json!({"payment_id":id,"status":"CONFIRMED","retry":"QUEUED"}),
    ))
}
async fn get_deposits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate_scoped(&state, &headers, Some("payments:read")).await?;
    let p = state
        .store
        .get_payment(merchant, &id)
        .await
        .map_err(map_store)?;
    let deposits = state.store.payment_deposits(p.id).await.map_err(db)?;
    Ok(Json(json!({"data":deposits})))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CreateClaimRequest {
    payment_id: String,
    transaction_hash: Option<String>,
    actual_chain: Option<String>,
    actual_asset: Option<String>,
    originating_wallet: Option<String>,
    recovery_destination: String,
    explanation: String,
}
async fn create_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateClaimRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let idem = idempotency_key(&headers)?;
    let request_hash = hex::encode(Sha256::digest(serde_json::to_vec(&req).map_err(internal)?));
    if let Some(existing) =
        reserve_idempotency_key(&state, merchant, "POST:/v1/claims", &idem, &request_hash).await?
    {
        return Ok((StatusCode::OK, Json(existing)));
    }
    let payment = state
        .store
        .get_payment(merchant, &req.payment_id)
        .await
        .map_err(map_store)?;
    let claimed_chain = req
        .actual_chain
        .as_deref()
        .map(ChainKey::from_str)
        .transpose()
        .map_err(|_| ApiError::bad("unsupported_chain", "invalid actual_chain"))?;
    if let Some(chain) = &claimed_chain {
        if *chain == ChainKey::Solana {
            return Err(ApiError::bad(
                "unsupported_network",
                "Solana wrong-chain recovery is not implemented",
            ));
        }
    }
    validate_evm_address(&req.recovery_destination)?;
    if let Some(wallet) = &req.originating_wallet {
        validate_evm_address(wallet)?;
    }
    if req.explanation.trim().len() < 3 {
        return Err(ApiError::bad(
            "invalid_explanation",
            "explanation is too short",
        ));
    }
    let claim_id = ClaimId::new();
    let public_id = format!("clm_{}", claim_id.0.simple());
    let initial = if req.originating_wallet.is_some() {
        ClaimState::AwaitingAuthorization
    } else {
        ClaimState::AwaitingEvidence
    };
    let claim = StoredClaim {
        id: claim_id,
        public_id: public_id.clone(),
        merchant_id: merchant,
        payment_id: payment.id,
        state: initial,
        expected_chain: payment.expected_chain.clone(),
        claimed_chain: claimed_chain.clone(),
        expected_asset: payment.expected_asset.symbol.clone(),
        claimed_asset: req.actual_asset.clone(),
        transaction_hash: req.transaction_hash.clone(),
        originating_wallet: req.originating_wallet.clone(),
        recovery_destination: req.recovery_destination.clone(),
        explanation: req.explanation.clone(),
    };
    state
        .store
        .create_claim(&claim)
        .await
        .map_err(|e| match &e {
            StoreError::Database(sqlx::Error::Database(dbe)) if dbe.is_unique_violation() => {
                ApiError::new(
                    StatusCode::CONFLICT,
                    "duplicate_claim",
                    "an active claim already exists for this payment/chain/transaction",
                )
            }
            _ => db(e),
        })?;
    state
        .store
        .set_payment_state(
            payment.id,
            flowpay_domain::PaymentState::ClaimPending,
            "claim_created",
            claimed_chain.as_ref(),
            req.transaction_hash.as_deref(),
        )
        .await
        .map_err(|_error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "claim_payment_transition_failed",
                "claim could not be synchronized with the payment state",
            )
        })?;
    let mut challenge_json = Value::Null;
    if let Some(wallet) = req.originating_wallet.as_deref() {
        let challenge_chain = claimed_chain.as_ref().unwrap_or(&payment.expected_chain);
        let challenge = new_wallet_challenge(
            claim_id,
            &payment.public_id,
            &req.recovery_destination,
            OffsetDateTime::now_utc(),
        );
        let challenge_id = state
            .store
            .store_wallet_challenge(
                claim_id,
                challenge_chain,
                wallet,
                &challenge.nonce,
                &challenge.message,
                challenge.expires_at,
            )
            .await
            .map_err(db)?;
        challenge_json = json!({"id":challenge_id,"message":challenge.message,"expires_at":challenge.expires_at.to_string(),"wallet":wallet});
    }
    let response = json!({"id":public_id,"payment_id":payment.public_id,"status":claim_state(initial),"wallet_challenge":challenge_json});
    store_idempotent_value(
        &state,
        merchant,
        "POST:/v1/claims",
        &idem,
        &request_hash,
        &response,
        "CLAIM",
        response["id"].as_str().unwrap_or_default(),
    )
    .await?;
    enqueue_event(
        &state,
        merchant,
        "claim.created",
        "CLAIM",
        response["id"].as_str().unwrap_or_default(),
        response.clone(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn get_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    claim_view(state,merchant,id).await
}

async fn claim_view(state:AppState,merchant:MerchantId,id:String)->Result<Json<Value>,ApiError>{
    let c = state
        .store
        .get_claim(merchant, &id)
        .await
        .map_err(map_store)?;
    let timeline_rows=sqlx::query("SELECT from_state,to_state,reason_code,actor_type,created_at FROM claim_state_transitions WHERE claim_id=$1 ORDER BY id").bind(c.id.0).fetch_all(state.store.pool()).await.map_err(db)?;
    let timeline=timeline_rows.into_iter().map(|r|json!({"from":r.try_get::<Option<String>,_>("from_state").ok().flatten(),"to":r.try_get::<String,_>("to_state").unwrap_or_default(),"reason":r.try_get::<String,_>("reason_code").unwrap_or_default(),"actor":r.try_get::<String,_>("actor_type").unwrap_or_default(),"at":r.try_get::<OffsetDateTime,_>("created_at").map(|v|v.to_string()).unwrap_or_default()})).collect::<Vec<_>>();
    let evidence=sqlx::query("SELECT id,evidence_type,text_content,content_sha256,authoritative,created_at FROM claim_evidence WHERE claim_id=$1 ORDER BY created_at").bind(c.id.0).fetch_all(state.store.pool()).await.map_err(db)?.into_iter().map(|r|json!({"id":format!("ev_{}",r.try_get::<Uuid,_>("id").unwrap_or_default().simple()),"type":r.try_get::<String,_>("evidence_type").unwrap_or_default(),"text":r.try_get::<Option<String>,_>("text_content").ok().flatten(),"sha256":r.try_get::<Option<String>,_>("content_sha256").ok().flatten(),"authoritative":r.try_get::<bool,_>("authoritative").unwrap_or(false)})).collect::<Vec<_>>();
    let tool_calls=sqlx::query("SELECT t.step_number,t.tool_name,t.input_redacted,t.output_redacted,t.status,t.error_class,t.started_at,t.completed_at FROM agent_tool_calls t JOIN agent_runs r ON r.id=t.agent_run_id WHERE r.claim_id=$1 ORDER BY r.started_at,t.step_number").bind(c.id.0).fetch_all(state.store.pool()).await.map_err(db)?.into_iter().map(|r|json!({"sequence":r.try_get::<i32,_>("step_number").unwrap_or_default(),"tool":r.try_get::<String,_>("tool_name").unwrap_or_default(),"input":r.try_get::<Value,_>("input_redacted").unwrap_or(Value::Null),"output":r.try_get::<Option<Value>,_>("output_redacted").ok().flatten(),"status":r.try_get::<String,_>("status").unwrap_or_default(),"error_class":r.try_get::<Option<String>,_>("error_class").ok().flatten()})).collect::<Vec<_>>();
    let decisions=sqlx::query("SELECT d.step_number,d.decision_summary,d.verification_summary,d.policy_effect,d.chosen_tool,d.created_at,r.policy_version FROM agent_decisions d JOIN agent_runs r ON r.id=d.agent_run_id WHERE r.claim_id=$1 ORDER BY d.created_at").bind(c.id.0).fetch_all(state.store.pool()).await.map_err(db)?.into_iter().map(|r|json!({"sequence":r.try_get::<i32,_>("step_number").unwrap_or_default(),"rationale":r.try_get::<String,_>("decision_summary").unwrap_or_default(),"verification":r.try_get::<Option<String>,_>("verification_summary").ok().flatten(),"policy_effect":r.try_get::<Option<String>,_>("policy_effect").ok().flatten(),"chosen_tool":r.try_get::<Option<String>,_>("chosen_tool").ok().flatten(),"policy_version":r.try_get::<String,_>("policy_version").unwrap_or_default()})).collect::<Vec<_>>();
    let runs=sqlx::query("SELECT public_id,agent_version,policy_version,status,final_disposition,model_provider,model_name,orchestration_mode,started_at,completed_at FROM agent_runs WHERE claim_id=$1 ORDER BY started_at")
        .bind(c.id.0).fetch_all(state.store.pool()).await.map_err(db)?.into_iter().map(|r|json!({
            "id":r.try_get::<String,_>("public_id").unwrap_or_default(),
            "agent_version":r.try_get::<String,_>("agent_version").unwrap_or_default(),
            "policy_version":r.try_get::<String,_>("policy_version").unwrap_or_default(),
            "status":r.try_get::<String,_>("status").unwrap_or_default(),
            "disposition":r.try_get::<Option<String>,_>("final_disposition").ok().flatten(),
            "model_provider":r.try_get::<Option<String>,_>("model_provider").ok().flatten(),
            "model_name":r.try_get::<Option<String>,_>("model_name").ok().flatten(),
            "mode":r.try_get::<String,_>("orchestration_mode").unwrap_or_else(|_|"DETERMINISTIC".into()),
            "started_at":r.try_get::<OffsetDateTime,_>("started_at").map(|v|v.to_string()).unwrap_or_default(),
            "completed_at":r.try_get::<Option<OffsetDateTime>,_>("completed_at").ok().flatten().map(|v|v.to_string())
        })).collect::<Vec<_>>();
    let recovery=sqlx::query("SELECT r.public_id,r.source_chain,r.asset_symbol,r.token_contract,r.amount_atomic::text AS amount_atomic,r.recovery_destination,r.receiver_deployment_required,r.estimated_gas_atomic::text AS estimated_gas,r.policy_version,r.policy_decision,r.simulation_status,r.risk_flags,r.plan_hash,a.public_id AS approval_id,a.status AS approval_status,e.tx_hash,e.state AS execution_status FROM recovery_plans r LEFT JOIN approvals a ON a.recovery_plan_id=r.id LEFT JOIN recovery_executions e ON e.recovery_plan_id=r.id WHERE r.claim_id=$1 ORDER BY r.created_at DESC LIMIT 1").bind(c.id.0).fetch_optional(state.store.pool()).await.map_err(db)?.map(|r|json!({"id":r.try_get::<String,_>("public_id").unwrap_or_default(),"source_chain":r.try_get::<String,_>("source_chain").unwrap_or_default(),"asset":r.try_get::<String,_>("asset_symbol").unwrap_or_default(),"asset_contract":r.try_get::<Option<String>,_>("token_contract").ok().flatten(),"amount_atomic":r.try_get::<String,_>("amount_atomic").unwrap_or_default(),"destination":r.try_get::<String,_>("recovery_destination").unwrap_or_default(),"receiver_deployment_required":r.try_get::<bool,_>("receiver_deployment_required").unwrap_or(false),"estimated_gas":r.try_get::<Option<String>,_>("estimated_gas").ok().flatten(),"policy":r.try_get::<String,_>("policy_version").unwrap_or_default(),"policy_decision":r.try_get::<String,_>("policy_decision").unwrap_or_default(),"simulation_status":r.try_get::<String,_>("simulation_status").unwrap_or_default(),"risk_flags":r.try_get::<Vec<String>,_>("risk_flags").unwrap_or_default(),"plan_hash":r.try_get::<String,_>("plan_hash").unwrap_or_default(),"approval_id":r.try_get::<Option<String>,_>("approval_id").ok().flatten(),"approval_status":r.try_get::<Option<String>,_>("approval_status").ok().flatten(),"recovery_tx":r.try_get::<Option<String>,_>("tx_hash").ok().flatten(),"execution_status":r.try_get::<Option<String>,_>("execution_status").ok().flatten()}));
    let latest_tool_output = |name: &str| {
        tool_calls
            .iter()
            .rev()
            .find(|call| call.get("tool").and_then(Value::as_str) == Some(name))
            .and_then(|call| call.get("output"))
    };
    let transaction_located = latest_tool_output("get_transaction").is_some_and(|v| {
        v.get("transaction").is_some() || v.get("ok").and_then(Value::as_bool) == Some(true)
    });
    let ownership_verified = latest_tool_output("verify_wallet_signature")
        .and_then(|v| v.get("authorization"))
        .and_then(|v| v.get("verified"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let checkout_address_verified = latest_tool_output("verify_counterfactual_address")
        .and_then(|v| v.get("counterfactual"))
        .is_some_and(|v| {
            v.get("matches").and_then(Value::as_bool) == Some(true)
                && v.get("factory_verified").and_then(Value::as_bool) == Some(true)
        });
    let funds_present = latest_tool_output("get_asset_balance")
        .or_else(|| latest_tool_output("get_token_balance"))
        .and_then(|v| v.get("balance_atomic"))
        .is_some_and(|v| match v {
            Value::String(s) => s != "0",
            Value::Number(n) => n.as_u64().is_some_and(|n| n > 0),
            _ => false,
        });
    let policy_passed = recovery
        .as_ref()
        .and_then(|v| v.get("policy_decision"))
        .and_then(Value::as_str)
        .is_some_and(|v| matches!(v, "ALLOWED" | "NEEDS_FUNDING"));
    let simulation_passed = recovery
        .as_ref()
        .and_then(|v| v.get("simulation_status"))
        .and_then(Value::as_str)
        == Some("SUCCEEDED");
    let investigation = json!({"transaction_located":transaction_located,"ownership_verified":ownership_verified,"checkout_address_verified":checkout_address_verified,"funds_present":funds_present,"policy_passed":policy_passed,"simulation_passed":simulation_passed});
    Ok(Json(
        json!({"id":c.public_id,"payment_id":format!("pay_{}",c.payment_id.0.simple()),"status":claim_state(c.state),"expected_chain":c.expected_chain,"actual_chain":c.claimed_chain,"transaction_hash":c.transaction_hash,"expected_asset":c.expected_asset,"actual_asset":c.claimed_asset,"originating_wallet":c.originating_wallet,"recovery_destination":c.recovery_destination,"explanation":c.explanation,"evidence":evidence,"timeline":timeline,"investigation":investigation,"agent":{"runs":runs,"tool_calls":tool_calls,"decisions":decisions},"recovery":recovery}),
    ))
}

#[derive(Debug, Deserialize)]
struct EvidenceRequest {
    evidence_type: String,
    text: Option<String>,
    filename: Option<String>,
    content_base64: Option<String>,
}
async fn add_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<EvidenceRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let claim = state
        .store
        .get_claim(merchant, &id)
        .await
        .map_err(map_store)?;
    let allowed = [
        "TX_HASH",
        "SCREENSHOT",
        "RECEIPT",
        "EXCHANGE_WITHDRAWAL",
        "WALLET_ADDRESS",
        "DOCUMENT",
        "TEXT",
    ];
    if !allowed.contains(&req.evidence_type.as_str()) {
        return Err(ApiError::bad(
            "invalid_evidence_type",
            "unsupported evidence_type",
        ));
    }
    let mut storage_key = None;
    let mut digest = None;
    if let Some(encoded) = req.content_base64.as_deref() {
        let bytes = B64
            .decode(encoded)
            .map_err(|_| ApiError::bad("invalid_evidence", "content_base64 is invalid"))?;
        if bytes.len() > 5 * 1024 * 1024 {
            return Err(ApiError::bad(
                "evidence_too_large",
                "evidence file limit is 5 MiB",
            ));
        }
        let sha = hex::encode(Sha256::digest(&bytes));
        let safe_name = req
            .filename
            .as_deref()
            .unwrap_or("evidence.bin")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            .collect::<String>();
        let dir = PathBuf::from(&state.config.evidence_dir).join(claim.id.0.to_string());
        tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
        let path = dir.join(format!("{}_{}", Uuid::now_v7().simple(), safe_name));
        tokio::fs::write(&path, &bytes).await.map_err(internal)?;
        storage_key = Some(path.to_string_lossy().to_string());
        digest = Some(sha);
    }
    let evidence_id = state
        .store
        .add_claim_evidence(
            claim.id,
            &req.evidence_type,
            req.text.as_deref(),
            storage_key.as_deref(),
            digest.as_deref(),
            "CUSTOMER",
        )
        .await
        .map_err(db)?;
    if claim.state == ClaimState::AwaitingEvidence {
        state
            .store
            .set_claim_state(
                claim.id,
                if claim.originating_wallet.is_some() {
                    ClaimState::AwaitingAuthorization
                } else {
                    ClaimState::Investigating
                },
                "evidence_received",
                "CUSTOMER",
            )
            .await
            .map_err(db)?;
    }
    Ok((
        StatusCode::CREATED,
        Json(json!({"id":evidence_id,"authoritative":false,"sha256":digest})),
    ))
}

#[derive(Debug, Deserialize)]
struct AuthorizeRequest {
    signature: String,
}
async fn authorize_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<AuthorizeRequest>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let claim = state
        .store
        .get_claim(merchant, &id)
        .await
        .map_err(map_store)?;
    let (challenge_id, wallet, nonce, message, expires_at) = state
        .store
        .latest_wallet_challenge(claim.id)
        .await
        .map_err(map_store)?;
    let challenge = WalletChallenge {
        claim_id: claim.id,
        nonce,
        message,
        expires_at,
    };
    let verified = verify_eip191_signature(
        &challenge,
        &wallet,
        &req.signature,
        OffsetDateTime::now_utc(),
    )
    .map_err(|e| ApiError::bad("signature_verification_failed", e.to_string()))?;
    state
        .store
        .mark_wallet_signature(challenge_id, &req.signature, verified.verified)
        .await
        .map_err(db)?;
    if !verified.verified {
        state
            .store
            .set_claim_state(
                claim.id,
                ClaimState::Escalated,
                "wallet_signature_mismatch",
                "CUSTOMER",
            )
            .await
            .map_err(db)?;
        let payment = state
            .store
            .get_payment_by_id(claim.payment_id)
            .await
            .map_err(map_store)?;
        if payment
            .state
            .can_transition_to(flowpay_domain::PaymentState::Escalated)
        {
            state
                .store
                .set_payment_state(
                    claim.payment_id,
                    flowpay_domain::PaymentState::Escalated,
                    "wallet_signature_mismatch",
                    claim.claimed_chain.as_ref(),
                    claim.transaction_hash.as_deref(),
                )
                .await
                .map_err(db)?;
        }
        return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY,"wallet_mismatch","signature did not recover to the claimed wallet; claim escalated without financial action"));
    }
    state
        .store
        .set_claim_state(
            claim.id,
            ClaimState::Investigating,
            "wallet_authorized",
            "CUSTOMER",
        )
        .await
        .map_err(db)?;
    Ok(Json(
        json!({"verified":true,"wallet":verified.recovered_address,"status":"INVESTIGATING"}),
    ))
}

async fn retry_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let claim = state
        .store
        .get_claim(merchant, &id)
        .await
        .map_err(map_store)?;
    if !matches!(
        claim.state,
        ClaimState::Escalated | ClaimState::NeedsMoreEvidence
    ) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "claim_not_retryable",
            "only an escalated or needs-more-evidence claim can be retried",
        ));
    }
    let payment = state
        .store
        .get_payment_by_id(claim.payment_id)
        .await
        .map_err(map_store)?;
    let has_approved_plan: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM approvals WHERE claim_id=$1 AND status='APPROVED' AND expires_at>now())",
    )
    .bind(claim.id.0)
    .fetch_one(state.store.pool())
    .await
    .map_err(db)?;
    if has_approved_plan {
        state
            .store
            .set_claim_state(
                claim.id,
                ClaimState::ApprovalPending,
                "resume_claimant_authorized_recovery",
                "SYSTEM",
            )
            .await
            .map_err(db)?;
        return Ok(Json(json!({"id":id,"status":"APPROVAL_PENDING"})));
    }
    if payment.state == PaymentState::Escalated {
        state
            .store
            .set_payment_state(
                claim.payment_id,
                PaymentState::ClaimPending,
                "operator_retry_after_system_fix",
                None,
                None,
            )
            .await
            .map_err(db)?;
    }
    state
        .store
        .set_claim_state(
            claim.id,
            ClaimState::Investigating,
            "operator_retry_after_system_fix",
            "SYSTEM",
        )
        .await
        .map_err(db)?;
    Ok(Json(json!({"id":id,"status":"INVESTIGATING"})))
}

async fn start_claim_investigation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let claim = state
        .store
        .get_claim(merchant, &id)
        .await
        .map_err(map_store)?;
    if !matches!(
        claim.state,
        ClaimState::AwaitingEvidence
            | ClaimState::AwaitingAuthorization
            | ClaimState::NeedsMoreEvidence
            | ClaimState::Escalated
    ) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "claim_not_investigatable",
            "claim is already being investigated or has a final disposition",
        ));
    }
    let payment = state
        .store
        .get_payment_by_id(claim.payment_id)
        .await
        .map_err(map_store)?;
    if payment.state.can_transition_to(PaymentState::ClaimPending) {
        state
            .store
            .set_payment_state(
                payment.id,
                PaymentState::ClaimPending,
                "internal_investigation_requested",
                claim.claimed_chain.as_ref(),
                claim.transaction_hash.as_deref(),
            )
            .await
            .map_err(db)?;
    }
    state
        .store
        .set_claim_state(
            claim.id,
            ClaimState::Investigating,
            "internal_investigation_requested",
            "SYSTEM",
        )
        .await
        .map_err(db)?;
    Ok(Json(json!({"id":id,"status":"INVESTIGATING"})))
}

async fn fund_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    if state.config.environment != "local" {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "test_mode_only",
            "gas funding endpoint is available only in local/test mode",
        ));
    }
    let claim = state
        .store
        .get_claim(merchant, &id)
        .await
        .map_err(map_store)?;
    let chain = claim
        .claimed_chain
        .clone()
        .ok_or_else(|| ApiError::bad("missing_chain", "claim has no actual chain"))?;
    let runtime = state
        .chains
        .get(&chain)
        .ok_or_else(|| ApiError::bad("unsupported_chain", "claimed chain is not configured"))?;
    let faucet = state.config.faucet_address.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "faucet_not_configured",
            "FLOWPAY_FAUCET_ADDRESS is required",
        )
    })?;
    let amount = "0x16345785d8a0000";
    let response:Value=state.http.post(&runtime.rpc_url).json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_sendTransaction","params":[{"from":faucet,"to":state.config.operator_address,"value":amount}]})).send().await.map_err(internal)?.json().await.map_err(internal)?;
    if let Some(error) = response.get("error") {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "faucet_transaction_failed",
            error.to_string(),
        ));
    }
    let tx_hash = response
        .get("result")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "faucet_transaction_failed",
                "missing transaction hash",
            )
        })?;
    sqlx::query("INSERT INTO test_gas_funding(claim_id,chain,destination,amount_atomic,tx_hash,state) VALUES($1,$2,$3,$4::numeric,$5,'SUBMITTED')").bind(claim.id.0).bind(chain.to_string()).bind(&state.config.operator_address).bind("100000000000000000").bind(tx_hash).execute(state.store.pool()).await.map_err(db)?;
    Ok(Json(
        json!({"status":"SUBMITTED","transaction_hash":tx_hash,"destination":state.config.operator_address}),
    ))
}

async fn approve_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let claim = state
        .store
        .get_claim(merchant, &id)
        .await
        .map_err(map_store)?;
    let mut tx = state.store.pool().begin().await.map_err(db)?;
    let row=sqlx::query("SELECT a.id,a.public_id,a.plan_hash,r.public_id AS plan_public_id FROM approvals a JOIN recovery_plans r ON r.id=a.recovery_plan_id WHERE a.claim_id=$1 AND a.status='PENDING' AND a.expires_at>now() ORDER BY a.created_at DESC LIMIT 1 FOR UPDATE").bind(claim.id.0).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||ApiError::bad("no_pending_approval","claim has no pending approval"))?;
    let approval_id: Uuid = row.try_get("id").map_err(internal)?;
    let public_id: String = row.try_get("public_id").map_err(internal)?;
    let plan_public_id: String = row.try_get("plan_public_id").map_err(internal)?;
    let changed=sqlx::query("UPDATE approvals SET status='APPROVED',approved_by=$2,approved_at=now() WHERE id=$1 AND status='PENDING'").bind(approval_id).bind(format!("merchant:{}",merchant.0)).execute(&mut *tx).await.map_err(db)?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "approval_race",
            "approval was already changed",
        ));
    }
    let event_id=enqueue_domain_event_tx(&mut tx,"flowpay.recovery","recovery.approved","RECOVERY_PLAN",&plan_public_id,json!({"claim_id":&id,"plan_id":&plan_public_id,"approval_id":&public_id,"approved_by":format!("merchant:{}",merchant.0)}),None,None).await.map_err(internal)?;
    enqueue_command_tx(
        &mut tx,
        "recovery.execute",
        "recovery.execute",
        "RECOVERY_PLAN",
        &plan_public_id,
        json!({"claim_id":&id,"plan_id":&plan_public_id,"approval_id":&public_id}),
        None,
        Some(event_id),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(
        json!({"approval_id":public_id,"plan_id":plan_public_id,"status":"APPROVED","note":"recovery worker will execute only after re-validating plan hash and simulation"}),
    ))
}

#[derive(Debug, Deserialize)]
struct CreateWebhookRequest {
    url: String,
    events: Option<Vec<String>>,
}
async fn create_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    validate_webhook_url(&req.url, &state.config.environment)?;
    let events = req.events.unwrap_or_default();
    let allowed = [
        "payment.created",
        "payment.detected",
        "payment.partially_paid",
        "payment.confirmed",
        "payment.completed",
        "payment.failed",
        "claim.created",
        "claim.recoverable",
        "claim.recovery_pending",
        "claim.recovered",
        "claim.rejected",
        "claim.escalated",
        "webhook.test",
    ];
    if events.iter().any(|e| !allowed.contains(&e.as_str())) {
        return Err(ApiError::bad(
            "invalid_webhook_event",
            "one or more event names are unsupported",
        ));
    }
    let signing_secret = flowpay_webhooks::generate_signing_secret();
    let sealed = flowpay_webhooks::encrypt_secret(
        &state.config.webhook_encryption_key,
        signing_secret.as_bytes(),
    )
    .map_err(internal)?;
    let id:Uuid=sqlx::query_scalar("INSERT INTO webhook_endpoints(merchant_id,url,signing_secret_ciphertext,subscribed_events) VALUES($1,$2,$3,$4) RETURNING id").bind(merchant.0).bind(&req.url).bind(sealed).bind(&events).fetch_one(state.store.pool()).await.map_err(db)?;
    Ok((
        StatusCode::CREATED,
        Json(
            json!({"id":format!("wh_{}",id.simple()),"url":req.url,"events":events,"enabled":true,"signing_secret":signing_secret,"warning":"Store this signing secret now; it is not returned again."}),
        ),
    ))
}
async fn list_webhooks(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let rows=sqlx::query("SELECT id,url,enabled,subscribed_events,created_at FROM webhook_endpoints WHERE merchant_id=$1 ORDER BY created_at DESC").bind(merchant.0).fetch_all(state.store.pool()).await.map_err(db)?;
    let data=rows.into_iter().map(|r|json!({"id":format!("wh_{}",r.try_get::<Uuid,_>("id").unwrap_or_default().simple()),"url":r.try_get::<String,_>("url").unwrap_or_default(),"enabled":r.try_get::<bool,_>("enabled").unwrap_or(false),"events":r.try_get::<Vec<String>,_>("subscribed_events").unwrap_or_default()})).collect::<Vec<_>>();
    Ok(Json(json!({"data":data})))
}
fn validate_webhook_url(value: &str, environment: &str) -> Result<(), ApiError> {
    let parsed = url::Url::parse(value)
        .map_err(|_| ApiError::bad("invalid_webhook_url", "webhook URL is invalid"))?;
    if environment != "local" && parsed.scheme() != "https" {
        return Err(ApiError::bad(
            "invalid_webhook_url",
            "HTTPS is required outside local mode",
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| ApiError::bad("invalid_webhook_url", "webhook URL has no host"))?
        .to_ascii_lowercase();
    if environment != "local"
        && (host == "localhost"
            || host == "::1"
            || host.ends_with(".local")
            || host.starts_with("127.")
            || host.starts_with("10.")
            || host.starts_with("192.168.")
            || host.starts_with("169.254.")
            || host.starts_with("172.16.")
            || host.starts_with("172.17.")
            || host.starts_with("172.18.")
            || host.starts_with("172.19.")
            || host.starts_with("172.2")
            || host.starts_with("172.30.")
            || host.starts_with("172.31.")
            || host.starts_with("fc")
            || host.starts_with("fd")
            || host.starts_with("fe80:"))
    {
        return Err(ApiError::bad(
            "invalid_webhook_url",
            "private/link-local webhook targets are blocked",
        ));
    }
    Ok(())
}

async fn test_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let event_id = enqueue_event(
        &state,
        merchant,
        "webhook.test",
        "PAYMENT",
        "test",
        json!({"test":true}),
    )
    .await?;
    Ok(Json(json!({"event_id":event_id,"queued":true})))
}

pub(crate) async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<MerchantId, ApiError> {
    authenticate_scoped(state, headers, None).await
}

pub(crate) async fn authenticate_dashboard(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<MerchantId, ApiError> {
    if bearer_token(headers).is_none_or(|token| token.starts_with("fp_oauth_")) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "dashboard_session_required",
            "this operation requires an authenticated dashboard session",
        ));
    }
    authenticate_scoped(state, headers, None).await
}

pub(crate) async fn authenticate_scoped(
    state: &AppState,
    headers: &HeaderMap,
    required_scope: Option<&str>,
) -> Result<MerchantId, ApiError> {
    // A dashboard session takes precedence over an API key: the browser holds
    // this instead, and it already resolves to exactly one merchant.
    if let Some(token) = bearer_token(headers) {
        if token.starts_with("fp_oauth_") {
            if required_scope.is_none() {
                return Err(ApiError::new(StatusCode::FORBIDDEN,"insufficient_scope","this operation requires a dashboard session or API key"));
            }
            return crate::oauth::validate_access(state, token, required_scope).await.map(|(merchant,_)| merchant);
        }
        let token_hash = crate::auth::hash_secret(token);
        let merchant: Option<Uuid> = sqlx::query_scalar(
            "UPDATE merchant_sessions SET last_seen_at=now() WHERE token_hash=$1 AND expires_at>now() RETURNING merchant_id",
        )
        .bind(&token_hash)
        .fetch_optional(state.store.pool())
        .await
        .map_err(db)?;
        let merchant = merchant.ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid_session",
                "session is invalid or has expired",
            )
        })?;
        let active: Option<bool> =
            sqlx::query_scalar("SELECT status='ACTIVE' FROM merchants WHERE id=$1")
                .bind(merchant)
                .fetch_optional(state.store.pool())
                .await
                .map_err(db)?;
        if active != Some(true) {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "merchant_disabled",
                "merchant account is disabled",
            ));
        }
        return Ok(MerchantId(merchant));
    }
    let key = match headers
        .get("x-flowpay-api-key")
        .and_then(|v| v.to_str().ok())
    {
        Some(k) => k,
        None if cfg!(debug_assertions)
            && state.config.environment == "local"
            && std::env::var("FLOWPAY_NGROK_ENABLED")
                .map_or(true, |value| !value.eq_ignore_ascii_case("true")) =>
        {
            // Unauthenticated access is available only in a debug local build.
            return Ok(state.config.default_dev_merchant());
        }
        _ => {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "missing_api_key",
                "x-flowpay-api-key is required",
            ));
        }
    };
    let prefix = key.split('.').next().unwrap_or(key);
    let record = state.store.api_key_by_prefix(prefix).await.map_err(|_| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "invalid API key",
        )
    })?;
    if !record.merchant_active {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "merchant_disabled",
            "merchant account is disabled",
        ));
    }
    if record
        .expires_at
        .is_some_and(|expires| expires <= OffsetDateTime::now_utc())
    {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "expired_api_key",
            "API key has expired",
        ));
    }
    if record.revoked {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "revoked_api_key",
            "API key has been revoked",
        ));
    }
    let computed = hex::encode(Sha256::digest(
        [state.config.api_key_pepper.as_bytes(), key.as_bytes()].concat(),
    ));
    if computed
        .as_bytes()
        .ct_eq(record.secret_hash.as_bytes())
        .unwrap_u8()
        != 1
    {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "invalid API key",
        ));
    }
    if let Some(scope) = required_scope {
        if !record.scopes.iter().any(|candidate| candidate == scope) {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "insufficient_scope",
                format!("API key requires {scope}"),
            ));
        }
    }
    Ok(record.merchant_id)
}
fn idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    let value = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError::bad(
                "missing_idempotency_key",
                "Idempotency-Key header is required",
            )
        })?;
    if value.len() < 8 || value.len() > 200 {
        return Err(ApiError::bad(
            "invalid_idempotency_key",
            "Idempotency-Key must be 8-200 characters",
        ));
    }
    Ok(value.to_owned())
}
async fn reserve_idempotency_key(
    state: &AppState,
    merchant: MerchantId,
    scope: &str,
    key: &str,
    request_hash: &str,
) -> Result<Option<Value>, ApiError> {
    let row=sqlx::query("SELECT request_hash,response_body,response_status FROM idempotency_keys WHERE merchant_id=$1 AND api_scope=$2 AND idempotency_key=$3").bind(merchant.0).bind(scope).bind(key).fetch_optional(state.store.pool()).await.map_err(db)?;
    if let Some(row) = row {
        let existing: String = row.try_get("request_hash").map_err(internal)?;
        if existing != request_hash {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "idempotency_conflict",
                "same Idempotency-Key was used with a different request",
            ));
        }
        let response: Option<Value> = row.try_get("response_body").map_err(internal)?;
        if let Some(response) = response {
            return Ok(Some(response));
        }
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "idempotency_in_progress",
            "an identical request is already being processed",
        ));
    }
    sqlx::query("INSERT INTO idempotency_keys(merchant_id,api_scope,idempotency_key,request_hash,response_status,response_body,expires_at) VALUES($1,$2,$3,$4,NULL,NULL,now()+interval '24 hours') ON CONFLICT DO NOTHING")
        .bind(merchant.0)
        .bind(scope)
        .bind(key)
        .bind(request_hash)
        .execute(state.store.pool())
        .await
        .map_err(db)?;
    let row = sqlx::query("SELECT request_hash,response_body FROM idempotency_keys WHERE merchant_id=$1 AND api_scope=$2 AND idempotency_key=$3")
        .bind(merchant.0).bind(scope).bind(key).fetch_one(state.store.pool()).await.map_err(db)?;
    let existing_hash: String = row.try_get("request_hash").map_err(internal)?;
    if existing_hash != request_hash {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "same Idempotency-Key was used with a different request",
        ));
    }
    let existing: Option<Value> = row.try_get("response_body").map_err(internal)?;
    Ok(existing)
}
async fn store_idempotent_response<T: Serialize>(
    state: &AppState,
    merchant: MerchantId,
    scope: &str,
    key: &str,
    request_hash: &str,
    response: &T,
    resource_type: &str,
    resource_id: &str,
) -> Result<(), ApiError> {
    store_idempotent_value(
        state,
        merchant,
        scope,
        key,
        request_hash,
        &serde_json::to_value(response).map_err(internal)?,
        resource_type,
        resource_id,
    )
    .await
}
async fn store_idempotent_value(
    state: &AppState,
    merchant: MerchantId,
    scope: &str,
    key: &str,
    request_hash: &str,
    response: &Value,
    resource_type: &str,
    resource_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE idempotency_keys SET response_status=201,response_body=$4,resource_type=$5,resource_public_id=$6 WHERE merchant_id=$1 AND api_scope=$2 AND idempotency_key=$3 AND response_body IS NULL")
        .bind(merchant.0)
        .bind(scope)
        .bind(key)
        .bind(response)
        .bind(resource_type)
        .bind(resource_id)
        .execute(state.store.pool())
        .await
        .map_err(db)?;
    sqlx::query("DELETE FROM idempotency_keys WHERE merchant_id=$1 AND api_scope=$2 AND idempotency_key=$3 AND expires_at<=now()")
        .bind(merchant.0)
        .bind(scope)
        .bind(key)
        .execute(state.store.pool())
        .await
        .map_err(db)?;
    sqlx::query("INSERT INTO idempotency_keys(merchant_id,api_scope,idempotency_key,request_hash,response_status,response_body,resource_type,resource_public_id,expires_at) VALUES($1,$2,$3,$4,201,$5,$6,$7,now()+interval '24 hours') ON CONFLICT DO NOTHING").bind(merchant.0).bind(scope).bind(key).bind(request_hash).bind(response).bind(resource_type).bind(resource_id).execute(state.store.pool()).await.map_err(db)?;
    Ok(())
}
async fn enqueue_event(
    state: &AppState,
    merchant: MerchantId,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    payload: Value,
) -> Result<String, ApiError> {
    state
        .store
        .enqueue_merchant_webhook_event(merchant, event_type, aggregate_type, aggregate_id, payload)
        .await
        .map_err(db)
}
fn payment_json(p: &Payment, checkout_base_url: &str, merchant_name: &str) -> Value {
    json!({"id":p.public_id,"address":p.checkout_address.value,"amount":p.expected_amount.to_decimal(p.expected_asset.decimals),"amount_atomic":p.expected_amount.to_string(),"asset":p.expected_asset.symbol,"chain":p.expected_chain.to_string(),"status":p.state.as_str(),"expires_at":p.expires_at.to_string(),"reference":p.reference,"merchant_name":merchant_name,"checkout_url":format!("{}/pay/{}",checkout_base_url.trim_end_matches('/'),p.public_id)})
}
fn parse_overpayment(value: Option<&str>) -> Result<OverpaymentPolicy, ApiError> {
    match value.unwrap_or("REQUIRE_REVIEW") {
        "ACCEPT_AND_RECORD" => Ok(OverpaymentPolicy::AcceptAndRecord),
        "REQUIRE_REVIEW" => Ok(OverpaymentPolicy::RequireReview),
        "REJECT_SETTLEMENT" => Ok(OverpaymentPolicy::RejectSettlement),
        _ => Err(ApiError::bad(
            "invalid_overpayment_policy",
            "unsupported overpayment policy",
        )),
    }
}
fn validate_evm_address(value: &str) -> Result<(), ApiError> {
    let raw = value.trim_start_matches("0x");
    if raw.len() != 40 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::bad(
            "invalid_address",
            "expected a 20-byte EVM address",
        ));
    }
    Ok(())
}
fn claim_state(s: ClaimState) -> String {
    s.as_str().to_owned()
}
pub(crate) fn map_store(e: StoreError) -> ApiError {
    match e {
        StoreError::NotFound => ApiError::not_found(),
        StoreError::Invalid(m) => ApiError::new(StatusCode::CONFLICT, "invalid_state", m),
        StoreError::ConcurrentUpdate => ApiError::new(
            StatusCode::CONFLICT,
            "concurrent_update",
            "resource changed; retry request",
        ),
        other => db(other),
    }
}
pub(crate) fn db<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "database_error",
        e.to_string(),
    )
}
fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        e.to_string(),
    )
}

/// Platform-side revenue reporting. This is the only place that reads the
/// platform's own ledger accounts, and it is gated by a separate admin
/// credential so merchant API keys can never see platform revenue.
// ---------------------------------------------------------------------------
// Merchant identity: signup, email verification, login, sessions, onboarding
// ---------------------------------------------------------------------------

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Normalizes an address for storage and comparison. Emails are matched
/// case-insensitively, which is what every mail provider effectively does.
fn normalize_email(raw: &str) -> Result<String, ApiError> {
    let email = raw.trim().to_ascii_lowercase();
    let valid = email.len() <= 254
        && email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    if !valid {
        return Err(ApiError::bad(
            "invalid_email",
            "enter a valid email address",
        ));
    }
    Ok(email)
}

fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.len() < 8 {
        return Err(ApiError::bad(
            "weak_password",
            "password must be at least 8 characters",
        ));
    }
    if password.len() > 200 {
        return Err(ApiError::bad(
            "invalid_password",
            "password must be at most 200 characters",
        ));
    }
    Ok(())
}

fn validate_business_name(name: &str) -> Result<String, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 120 {
        return Err(ApiError::bad(
            "invalid_business_name",
            "business name must be 1-120 characters",
        ));
    }
    Ok(trimmed.to_owned())
}

/// Mints a fresh session row and returns the bearer token. Only the hash is
/// persisted, so the raw token exists solely in the response.
pub(crate) async fn create_session(
    state: &AppState,
    merchant: Uuid,
    user_agent: Option<String>,
) -> Result<(String, OffsetDateTime), ApiError> {
    let token = crate::auth::new_session_token();
    let token_hash = crate::auth::hash_secret(&token);
    let expires_at =
        OffsetDateTime::now_utc() + Duration::hours(state.config.auth_session_ttl_hours.max(1));
    sqlx::query(
        "INSERT INTO merchant_sessions(id,merchant_id,token_hash,user_agent,expires_at) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(Uuid::now_v7())
    .bind(merchant)
    .bind(&token_hash)
    .bind(user_agent.as_deref())
    .bind(expires_at)
    .execute(state.store.pool())
    .await
    .map_err(db)?;
    Ok((token, expires_at))
}

fn merchant_json(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.try_get::<Uuid,_>("id").map(|v| v.to_string()).unwrap_or_default(),
        "email": row.try_get::<Option<String>,_>("email").ok().flatten(),
        "business_name": row.try_get::<String,_>("name").unwrap_or_default(),
        "contact_name": row.try_get::<Option<String>,_>("contact_name").ok().flatten(),
        "public_id": row.try_get::<String,_>("public_id").unwrap_or_default(),
        "status": row.try_get::<String,_>("status").unwrap_or_default(),
        "email_verified": row.try_get::<Option<OffsetDateTime>,_>("email_verified_at").ok().flatten().is_some(),
        "onboarding_completed": row.try_get::<Option<OffsetDateTime>,_>("onboarding_completed_at").ok().flatten().is_some(),
        "settlement_address": row.try_get::<Option<String>,_>("evm_settlement_address").ok().flatten(),
        "created_at": row.try_get::<OffsetDateTime,_>("created_at").map(|v| v.unix_timestamp() * 1000).unwrap_or_default(),
    })
}

const MERCHANT_COLUMNS: &str = "id,email,name,contact_name,public_id,status,email_verified_at,onboarding_completed_at,evm_settlement_address,created_at";

pub(crate) async fn load_merchant(state: &AppState, merchant: Uuid) -> Result<Value, ApiError> {
    let row = sqlx::query(&format!(
        "SELECT {MERCHANT_COLUMNS} FROM merchants WHERE id=$1"
    ))
    .bind(merchant)
    .fetch_optional(state.store.pool())
    .await
    .map_err(db)?
    .ok_or_else(ApiError::not_found)?;
    Ok(merchant_json(&row))
}


async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let Some(token) = bearer_token(&headers) else {
        return Ok(Json(json!({"signed_out": true})));
    };
    let token_hash = crate::auth::hash_secret(token);
    sqlx::query("DELETE FROM merchant_sessions WHERE token_hash=$1")
        .bind(&token_hash)
        .execute(state.store.pool())
        .await
        .map_err(db)?;
    Ok(Json(json!({"signed_out": true})))
}

async fn current_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    Ok(Json(json!({ "merchant": load_merchant(&state, merchant.0).await? })))
}

#[derive(Deserialize)]
struct SettlementWalletRequest { address: String }

async fn configure_settlement_wallet(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<SettlementWalletRequest>) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate_dashboard(&state, &headers).await?;
    let address = input.address.trim();
    if address.len()!=42 || !address.starts_with("0x") || !address[2..].bytes().all(|byte|byte.is_ascii_hexdigit()) || address[2..].bytes().all(|byte|byte==b'0') {
        return Err(ApiError::bad("invalid_settlement_address","Enter a valid, non-zero EVM wallet address."));
    }
    let updated = sqlx::query("UPDATE merchants SET evm_settlement_address=$2,updated_at=now() WHERE id=$1 AND (evm_settlement_address IS NULL OR btrim(evm_settlement_address)='' OR lower(evm_settlement_address)=lower($2))")
        .bind(merchant.0).bind(address).execute(state.store.pool()).await.map_err(db)?;
    if updated.rows_affected()!=1 {
        return Err(ApiError::new(StatusCode::CONFLICT,"settlement_wallet_configured","A settlement wallet is already configured."));
    }
    Ok(Json(json!({"settlement_address":address})))
}

#[derive(Debug, Deserialize)]
struct OnboardingRequest {
    business_name: String,
    /// Only required by the on-chain rails. Bank-transfer merchants never touch
    /// an EVM wallet, so signup must not block on one.
    #[serde(default)]
    evm_settlement_address: Option<String>,
    contact_name: Option<String>,
    #[serde(default)]
    require_passkey: bool,
    recovery_email: Option<String>,
}

async fn complete_onboarding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<OnboardingRequest>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    if req.require_passkey {
        crate::passkeys::require_registered(&state, merchant.0).await?;
    }
    let business_name = validate_business_name(&req.business_name)?;
    let email=req.recovery_email.as_deref().map(str::trim).filter(|v|!v.is_empty()).map(str::to_ascii_lowercase);
    if let Some(value)=&email{
        let parts:Vec<_>=value.split('@').collect();
        if value.len()>254||value.chars().any(char::is_whitespace)||parts.len()!=2||parts[0].is_empty()||!parts[1].contains('.'){
            return Err(ApiError::bad("invalid_email","Enter a valid recovery email address."));
        }
    }
    let address = req
        .evm_settlement_address
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(address) = address {
        let valid_address = address.len() == 42
            && address.starts_with("0x")
            && address[2..].bytes().all(|byte| byte.is_ascii_hexdigit());
        if !valid_address {
            return Err(ApiError::bad(
                "invalid_settlement_address",
                "settlement address must be a 0x-prefixed EVM address",
            ));
        }
    }
    let contact_name = req
        .contact_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(120).collect::<String>());
    sqlx::query(
        "UPDATE merchants SET name=$2,contact_name=coalesce($3,contact_name),evm_settlement_address=coalesce($4,evm_settlement_address),email=coalesce($5,email),email_verified_at=CASE WHEN $5::text IS NOT NULL AND email IS DISTINCT FROM $5 THEN NULL ELSE email_verified_at END,onboarding_completed_at=coalesce(onboarding_completed_at,now()),updated_at=now() WHERE id=$1",
    )
    .bind(merchant.0)
    .bind(&business_name)
    .bind(contact_name.as_deref())
    .bind(address)
    .bind(email)
    .execute(state.store.pool())
    .await
    .map_err(db)?;
    // Mint a first API key so the merchant can drive the API from their own
    // backend. Returned once, exactly like the dashboard's key creation flow.
    let environment = if state.config.environment == "local" {
        "test"
    } else {
        "live"
    };
    let prefix = format!(
        "fp_{}_{}",
        environment,
        &Uuid::now_v7().simple().to_string()[..10]
    );
    let secret = format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple());
    let full = format!("{prefix}.{secret}");
    let hash = hex::encode(Sha256::digest(
        [state.config.api_key_pepper.as_bytes(), full.as_bytes()].concat(),
    ));
    let existing: i64 =
        sqlx::query_scalar("SELECT count(*) FROM api_keys WHERE merchant_id=$1 AND revoked_at IS NULL")
            .bind(merchant.0)
            .fetch_one(state.store.pool())
            .await
            .map_err(db)?;
    let api_key = if existing > 0 {
        None
    } else {
        sqlx::query(
            "INSERT INTO api_keys(merchant_id,label,public_prefix,secret_hash) VALUES($1,$2,$3,$4)",
        )
        .bind(merchant.0)
        .bind("Default key")
        .bind(&prefix)
        .bind(&hash)
        .execute(state.store.pool())
        .await
        .map_err(db)?;
        Some(full)
    };
    Ok(Json(json!({
        "merchant": load_merchant(&state, merchant.0).await?,
        "api_key": api_key,
    })))
}

async fn admin_revenue(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let expected = state.config.admin_key.as_deref().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_IMPLEMENTED,
            "admin_not_configured",
            "admin access is not configured",
        )
    })?;
    let supplied = headers
        .get("x-flowpay-admin-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if supplied.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() != 1 {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_admin_key",
            "invalid admin key",
        ));
    }

    // Platform ledger balances, read from the ledger of record.
    let mut accounts = Vec::new();
    for address in ["platform:revenue", "platform:payment-fees"] {
        let encoded = address.replace(':', "%3A");
        let url = format!(
            "{}/api/ledger/v2/{}/accounts/{}?expand=volumes",
            state.config.formance_base_url, state.config.formance_ledger, encoded
        );
        let mut request = state
            .http
            .get(url)
            .timeout(StdDuration::from_secs(10));
        if let Some(token) = state.config.formance_token.as_deref() {
            request = request.bearer_auth(token);
        }
        let balance = match request.send().await {
            Ok(response) if response.status().is_success() => {
                match response.json::<Value>().await {
                    Ok(body) => body
                        .pointer("/data/volumes/NGN~12/balance")
                        .map(|value| value.to_string()),
                    Err(_) => None,
                }
            }
            _ => None,
        };
        accounts.push(json!({"account": address, "balance_atomic": balance}));
    }

    let totals = sqlx::query("SELECT count(*) FILTER (WHERE state='COMPLETED')::bigint AS completed,coalesce(sum(expected_amount_atomic) FILTER (WHERE state='COMPLETED'),0)::text AS collected,coalesce(sum(merchant_amount_atomic) FILTER (WHERE state='COMPLETED'),0)::text AS merchant_credited,coalesce(sum(platform_fee_atomic) FILTER (WHERE state='COMPLETED'),0)::text AS platform_fees,coalesce(sum(provider_fee_atomic) FILTER (WHERE state='COMPLETED'),0)::text AS provider_fees FROM payments WHERE expected_chain='custom:flutterwave_ngn'")
        .fetch_one(state.store.pool())
        .await
        .map_err(db)?;
    Ok(Json(json!({
        "environment": state.config.environment,
        "platform_fee_atomic": state.config.ngn_platform_fee_atomic.to_string(),
        "provider_fee_bps": state.config.flutterwave_fee_bps.to_string(),
        "provider_fee_vat_bps": state.config.flutterwave_fee_vat_bps.to_string(),
        "ledger": accounts,
        "ngn": {
            "completed_payments": totals.try_get::<i64,_>("completed").unwrap_or_default(),
            "collected_atomic": totals.try_get::<String,_>("collected").unwrap_or_default(),
            "merchant_credited_atomic": totals.try_get::<String,_>("merchant_credited").unwrap_or_default(),
            "platform_fee_atomic": totals.try_get::<String,_>("platform_fees").unwrap_or_default(),
            "provider_fee_atomic": totals.try_get::<String,_>("provider_fees").unwrap_or_default(),
        }
    })))
}

async fn get_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let settlement: Option<String> =
        sqlx::query_scalar("SELECT evm_settlement_address FROM merchants WHERE id=$1")
            .bind(merchant.0)
            .fetch_one(state.store.pool())
            .await
            .map_err(db)?;
    let settlement = settlement.ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "settlement_not_configured",
            "merchant settlement address is not configured",
        )
    })?;
    let rows=sqlx::query("SELECT DISTINCT chain,symbol,token_contract,decimals FROM chain_assets WHERE enabled=true AND purpose IN ('PAYMENT','RECOVERY','BOTH') AND token_contract IS NOT NULL ORDER BY chain,symbol")
        .fetch_all(state.store.pool()).await.map_err(db)?;
    // Each balance is an independent RPC call. Running them together keeps the
    // dashboard's critical path at roughly one round trip instead of one per
    // enabled chain and token, which is what made this endpoint the slowest
    // part of every dashboard render.
    let mut pending = Vec::new();
    for row in rows {
        let chain_name: String = row.try_get("chain").map_err(internal)?;
        let Ok(chain) = ChainKey::from_str(&chain_name) else {
            continue;
        };
        let Some(runtime) = state.chains.get(&chain) else {
            continue;
        };
        let token: String = row.try_get("token_contract").map_err(internal)?;
        let symbol: String = row.try_get("symbol").map_err(internal)?;
        let decimals: i16 = row.try_get("decimals").map_err(internal)?;
        let adapter = runtime.adapter.clone();
        let settlement = settlement.clone();
        pending.push(async move {
            match adapter.token_balance(&token,&settlement).await {
                Ok(amount)=>json!({"chain":chain,"symbol":symbol,"contract":token,"decimals":decimals,"amount_atomic":amount.to_string(),"amount":amount.to_decimal(decimals as u8)}),
                Err(error)=>json!({"chain":chain,"symbol":symbol,"contract":token,"decimals":decimals,"error":error.to_string()}),
            }
        });
    }
    let balances = futures_util::future::join_all(pending).await;
    let payment_counts=sqlx::query("SELECT count(*)::bigint AS total,count(*) FILTER(WHERE state='COMPLETED')::bigint AS completed FROM payments WHERE merchant_id=$1")
        .bind(merchant.0).fetch_one(state.store.pool()).await.map_err(db)?;
    let claim_counts=sqlx::query("SELECT count(*) FILTER(WHERE state NOT IN ('RECOVERED','REJECTED','NOT_RECOVERABLE'))::bigint AS open,count(*) FILTER(WHERE state IN ('RECOVERABLE','APPROVAL_PENDING','RECOVERY_PENDING'))::bigint AS actionable FROM claims WHERE merchant_id=$1")
        .bind(merchant.0).fetch_one(state.store.pool()).await.map_err(db)?;
    Ok(Json(json!({
        "settlement_address":settlement,
        "balances":balances,
        "payments":{"total":payment_counts.try_get::<i64,_>("total").unwrap_or_default(),"completed":payment_counts.try_get::<i64,_>("completed").unwrap_or_default()},
        "claims":{"open":claim_counts.try_get::<i64,_>("open").unwrap_or_default(),"actionable":claim_counts.try_get::<i64,_>("actionable").unwrap_or_default()},
        "agent_mode":state.config.agent_mode,
        "agent_provider":if state.config.agent_mode.eq_ignore_ascii_case("model"){Some(state.config.model_provider.clone())}else{None},
        "agent_model":if state.config.agent_mode.eq_ignore_ascii_case("model"){Some(state.config.openai_model.clone())}else{None},
        "environment":state.config.environment
    })))
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    limit: Option<i64>,
    cursor: Option<String>,
}
async fn list_payments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate_scoped(&state, &headers, Some("payments:read")).await?;
    let limit = q.limit.unwrap_or(25).clamp(1, 100);
    let rows=sqlx::query("SELECT p.public_id,p.merchant_reference,p.expected_chain,p.expected_asset_symbol,p.expected_asset_decimals,p.expected_amount_atomic::text AS amount_atomic,c.address AS checkout_address,p.state,p.expires_at,p.created_at FROM payments p JOIN checkout_addresses c ON c.payment_id=p.id AND c.chain=p.expected_chain WHERE p.merchant_id=$1 AND ($2::text IS NULL OR p.created_at < COALESCE((SELECT created_at FROM payments p2 WHERE p2.merchant_id=$1 AND p2.public_id=$2),now())) ORDER BY p.created_at DESC LIMIT $3")
      .bind(merchant.0).bind(q.cursor.as_deref()).bind(limit).fetch_all(state.store.pool()).await.map_err(db)?;
    let mut data = Vec::with_capacity(rows.len());
    for r in &rows {
        let atomic: String = r.try_get("amount_atomic").map_err(internal)?;
        let decimals: i16 = r.try_get("expected_asset_decimals").map_err(internal)?;
        let amount = AtomicAmount::from_str(&atomic).map_err(internal)?;
        data.push(json!({"id":r.try_get::<String,_>("public_id").map_err(internal)?,"reference":r.try_get::<Option<String>,_>("merchant_reference").map_err(internal)?,"chain":r.try_get::<String,_>("expected_chain").map_err(internal)?,"asset":r.try_get::<String,_>("expected_asset_symbol").map_err(internal)?,"amount":amount.to_decimal(decimals as u8),"amount_atomic":atomic,"address":r.try_get::<String,_>("checkout_address").map_err(internal)?,"status":r.try_get::<String,_>("state").map_err(internal)?,"expires_at":r.try_get::<OffsetDateTime,_>("expires_at").map_err(internal)?.to_string()}));
    }
    let next_cursor = if rows.len() as i64 == limit {
        rows.last()
            .and_then(|r| r.try_get::<String, _>("public_id").ok())
    } else {
        None
    };
    Ok(Json(json!({"data":data,"next_cursor":next_cursor})))
}

async fn list_claims(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let limit = q.limit.unwrap_or(25).clamp(1, 100);
    let rows=sqlx::query("SELECT c.public_id,c.state,c.claimed_chain,c.claimed_asset,c.claimed_transaction_hash,c.recovery_destination,c.created_at,p.public_id AS payment_public_id FROM claims c JOIN payments p ON p.id=c.payment_id WHERE c.merchant_id=$1 AND ($2::text IS NULL OR c.created_at < COALESCE((SELECT created_at FROM claims c2 WHERE c2.merchant_id=$1 AND c2.public_id=$2),now())) ORDER BY c.created_at DESC LIMIT $3")
      .bind(merchant.0).bind(q.cursor.as_deref()).bind(limit).fetch_all(state.store.pool()).await.map_err(db)?;
    let data=rows.iter().map(|r|json!({"id":r.try_get::<String,_>("public_id").unwrap_or_default(),"payment_id":r.try_get::<String,_>("payment_public_id").unwrap_or_default(),"status":r.try_get::<String,_>("state").unwrap_or_default(),"actual_chain":r.try_get::<Option<String>,_>("claimed_chain").ok().flatten(),"actual_asset":r.try_get::<Option<String>,_>("claimed_asset").ok().flatten(),"transaction_hash":r.try_get::<Option<String>,_>("claimed_transaction_hash").ok().flatten(),"recovery_destination":r.try_get::<String,_>("recovery_destination").unwrap_or_default()})).collect::<Vec<_>>();
    let next_cursor = if rows.len() as i64 == limit {
        rows.last()
            .and_then(|r| r.try_get::<String, _>("public_id").ok())
    } else {
        None
    };
    Ok(Json(json!({"data":data,"next_cursor":next_cursor})))
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    environment: Option<String>,
}
async fn list_api_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let rows=sqlx::query("SELECT id,label,public_prefix,created_at,last_used_at,revoked_at FROM api_keys WHERE merchant_id=$1 ORDER BY created_at DESC").bind(merchant.0).fetch_all(state.store.pool()).await.map_err(db)?;
    Ok(Json(
        json!({"data":rows.iter().map(|r|json!({"id":format!("key_{}",r.try_get::<Uuid,_>("id").unwrap_or_default().simple()),"name":r.try_get::<String,_>("label").unwrap_or_default(),"prefix":r.try_get::<String,_>("public_prefix").unwrap_or_default(),"permissions":["payments:read","payments:write","webhooks:read","webhooks:write"],"created_at":r.try_get::<OffsetDateTime,_>("created_at").ok().map(|v|v.unix_timestamp()*1000),"last_used_at":r.try_get::<Option<OffsetDateTime>,_>("last_used_at").ok().flatten().map(|v|v.unix_timestamp()*1000),"revoked":r.try_get::<Option<OffsetDateTime>,_>("revoked_at").ok().flatten().is_some()})).collect::<Vec<_>>() }),
    ))
}
async fn create_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    if req.name.trim().is_empty() || req.name.len() > 80 {
        return Err(ApiError::bad(
            "invalid_name",
            "key name must be 1-80 characters",
        ));
    }
    let environment =
        req.environment
            .as_deref()
            .unwrap_or(if state.config.environment == "local" {
                "test"
            } else {
                "live"
            });
    if !matches!(environment, "live" | "test") {
        return Err(ApiError::bad(
            "invalid_environment",
            "environment must be live or test",
        ));
    }
    let prefix = format!(
        "fp_{}_{}",
        environment,
        &Uuid::now_v7().simple().to_string()[..10]
    );
    let secret = format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple());
    let full = format!("{prefix}.{secret}");
    let hash = hex::encode(Sha256::digest(
        [state.config.api_key_pepper.as_bytes(), full.as_bytes()].concat(),
    ));
    let id:Uuid=sqlx::query_scalar("INSERT INTO api_keys(merchant_id,label,public_prefix,secret_hash) VALUES($1,$2,$3,$4) RETURNING id").bind(merchant.0).bind(req.name.trim()).bind(&prefix).bind(hash).fetch_one(state.store.pool()).await.map_err(db)?;
    Ok((
        StatusCode::CREATED,
        Json(
            json!({"id":format!("key_{}",id.simple()),"public_key":prefix,"secret_key":secret,"api_key":full,"warning":"The secret key and complete API credential are shown once."}),
        ),
    ))
}
async fn revoke_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let raw = id
        .strip_prefix("key_")
        .ok_or_else(|| ApiError::bad("invalid_key_id", "invalid API key id"))?;
    if raw.len() != 32 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::bad("invalid_key_id", "invalid API key id"));
    }
    let uuid = Uuid::parse_str(&format!(
        "{}-{}-{}-{}-{}",
        &raw[0..8],
        &raw[8..12],
        &raw[12..16],
        &raw[16..20],
        &raw[20..32]
    ))
    .map_err(|_| ApiError::bad("invalid_key_id", "invalid API key id"))?;
    let result=sqlx::query("UPDATE api_keys SET revoked_at=now() WHERE id=$1 AND merchant_id=$2 AND revoked_at IS NULL").bind(uuid).bind(merchant.0).execute(state.store.pool()).await.map_err(db)?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"id":id,"revoked":true})))
}

async fn list_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let rows=sqlx::query("SELECT id,actor_type,actor_id,action,request_id,correlation_id,chain,tx_hash,outcome,metadata_redacted,occurred_at FROM audit_logs WHERE merchant_id=$1 ORDER BY id DESC LIMIT $2").bind(merchant.0).bind(limit).fetch_all(state.store.pool()).await.map_err(db)?;
    Ok(Json(
        json!({"data":rows.iter().map(|r|json!({"actor":r.try_get::<String,_>("actor_type").unwrap_or_default(),"actor_id":r.try_get::<Option<String>,_>("actor_id").ok().flatten(),"action":r.try_get::<String,_>("action").unwrap_or_default(),"request_id":r.try_get::<Option<String>,_>("request_id").ok().flatten(),"correlation_id":r.try_get::<Option<String>,_>("correlation_id").ok().flatten(),"chain":r.try_get::<Option<String>,_>("chain").ok().flatten(),"transaction_hash":r.try_get::<Option<String>,_>("tx_hash").ok().flatten(),"outcome":r.try_get::<String,_>("outcome").unwrap_or_default(),"details":r.try_get::<Value,_>("metadata_redacted").unwrap_or(Value::Null),"created_at":r.try_get::<OffsetDateTime,_>("occurred_at").map(|v|v.to_string()).unwrap_or_default()})).collect::<Vec<_>>() }),
    ))
}

#[derive(Debug, Deserialize)]
struct AgentChatRequest {
    payment_id: String,
    session_id: Option<String>,
    email: Option<String>,
    messages: Vec<AgentChatMessage>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct AgentChatMessage {
    role: String,
    content: String,
}

fn find_native_transfer_in_trace(
    trace: &Value,
    checkout_address: &str,
) -> Option<(String, String)> {
    let destination_matches = trace
        .get("to")
        .and_then(Value::as_str)
        .is_some_and(|address| address.eq_ignore_ascii_case(checkout_address));
    let transferred_value = trace
        .get("value")
        .and_then(Value::as_str)
        .and_then(|value| value.strip_prefix("0x"))
        .and_then(|value| u128::from_str_radix(value, 16).ok())
        .unwrap_or_default();
    if destination_matches && transferred_value > 0 {
        return Some((
            trace
                .get("from")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            trace
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        ));
    }
    trace
        .get("calls")
        .and_then(Value::as_array)
        .and_then(|calls| {
            calls
                .iter()
                .find_map(|call| find_native_transfer_in_trace(call, checkout_address))
        })
}

#[derive(Debug, Clone)]
struct ResolvedTransaction {
    effective_sender: String,
    effective_destination: String,
    value_hex: String,
    used_internal_call: bool,
}

async fn resolve_transaction_tool(
    state: &AppState,
    chain: &ChainConfig,
    tx_hash: &str,
    checkout_address: &str,
) -> Result<Option<ResolvedTransaction>, ApiError> {
    let rpc_payload = state
        .http
        .post(&chain.rpc_url)
        .json(
            &json!({"jsonrpc":"2.0","id":1,"method":"eth_getTransactionByHash","params":[tx_hash]}),
        )
        .send()
        .await
        .map_err(internal)?
        .json::<Value>()
        .await
        .map_err(internal)?;
    let transaction = rpc_payload
        .get("result")
        .filter(|value| !value.is_null())
        .ok_or_else(|| {
            ApiError::bad(
                "transaction_not_found",
                "transaction was not found on the claimed network",
            )
        })?;
    let top_level_to = transaction
        .get("to")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if top_level_to.eq_ignore_ascii_case(checkout_address) {
        return Ok(Some(ResolvedTransaction {
            effective_sender: transaction
                .get("from")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            effective_destination: checkout_address.to_owned(),
            value_hex: transaction
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("0x0")
                .to_owned(),
            used_internal_call: false,
        }));
    }
    let trace_payload = state
        .http
        .post(&chain.rpc_url)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"debug_traceTransaction","params":[tx_hash,{"tracer":"callTracer"}]}))
        .send()
        .await
        .map_err(internal)?
        .json::<Value>()
        .await
        .map_err(internal)?;
    Ok(trace_payload
        .get("result")
        .and_then(|trace| find_native_transfer_in_trace(trace, checkout_address))
        .map(|(sender, value_hex)| ResolvedTransaction {
            effective_sender: sender,
            effective_destination: checkout_address.to_owned(),
            value_hex,
            used_internal_call: true,
        }))
}

/// Lightweight agent chat endpoint for the checkout page.
/// Uses Ollama to triage wrong-asset/wrong-chain claims conversationally.
/// When enough information is collected, creates a claim and returns the result.
async fn agent_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AgentChatRequest>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    // Load the payment to get context
    let payment = state
        .store
        .get_payment(merchant, &req.payment_id)
        .await
        .map_err(map_store)?;
    agent_chat_for_payment(state, req, payment).await
}

async fn public_agent_chat(State(state):State<AppState>,Path(id):Path<String>,Json(mut req):Json<AgentChatRequest>)->Result<Json<Value>,ApiError>{
    req.payment_id=id;
    let payment=state.store.get_payment_by_public_id(&req.payment_id).await.map_err(map_store)?;
    let active:bool=sqlx::query_scalar("SELECT status='ACTIVE' FROM merchants WHERE id=$1").bind(payment.merchant_id.0).fetch_one(state.store.pool()).await.map_err(db)?;
    if !active{return Err(ApiError::new(StatusCode::NOT_FOUND,"not_found","Checkout is unavailable."));}
    agent_chat_for_payment(state,req,payment).await
}

#[derive(Deserialize)]
struct AgentStatusQuery{claim_id:String,session_id:String}
async fn public_agent_status(State(state):State<AppState>,Path(id):Path<String>,Query(query):Query<AgentStatusQuery>)->Result<Json<Value>,ApiError>{
    let payment=state.store.get_payment_by_public_id(&id).await.map_err(map_store)?;
    let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_chat_intakes a JOIN claims c ON c.id=a.claim_id WHERE a.payment_id=$1 AND a.session_id=$2 AND c.public_id=$3)")
        .bind(payment.id.0).bind(query.session_id).bind(&query.claim_id).fetch_one(state.store.pool()).await.map_err(db)?;
    if !allowed{return Err(ApiError::new(StatusCode::NOT_FOUND,"not_found","Investigation not found for this conversation."));}
    let claim=claim_view(state,payment.merchant_id,query.claim_id).await?.0;
    let disposition=claim["agent"]["runs"].as_array().and_then(|runs|runs.last()).and_then(|run|run.get("final_disposition")).cloned().unwrap_or(Value::Null);
    let verdict=match claim["status"].as_str(){Some("RECOVERED")=>json!("RECOVERED"),Some("RECOVERY_PENDING")=>json!("RECOVERY_PENDING"),_=>disposition};
    Ok(Json(json!({"claim_id":claim["id"],"status":claim["status"],"verdict":verdict,"investigation":claim["investigation"],"recovery":claim["recovery"]})))
}

async fn agent_chat_for_payment(state:AppState,req:AgentChatRequest,payment:Payment)->Result<Json<Value>,ApiError>{
    let merchant=payment.merchant_id;
    if req.messages.is_empty()||req.messages.len()>30||req.messages.iter().map(|m|m.content.len()).sum::<usize>()>12000{
        return Err(ApiError::bad("invalid_chat_messages","Keep the conversation under 30 messages and 12000 characters."));
    }
    let supplied_email = req
        .email
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let email = if supplied_email.contains('@') {
        supplied_email
    } else {
        req.messages
            .iter()
            .rev()
            .filter(|message| message.role.eq_ignore_ascii_case("user"))
            .flat_map(|message| message.content.split_whitespace().rev())
            .map(|word| {
                word.trim_matches(|character: char| {
                    !character.is_ascii_alphanumeric()
                        && !matches!(character, '@' | '.' | '_' | '+' | '-')
                })
                .to_ascii_lowercase()
            })
            .find(|candidate| {
                let mut parts = candidate.split('@');
                let local = parts.next().unwrap_or_default();
                let domain = parts.next().unwrap_or_default();
                !local.is_empty()
                    && domain.contains('.')
                    && parts.next().is_none()
                    && candidate.len() <= 254
            })
            .unwrap_or_default()
    };
    let email_verified = email.contains('@') && email.len() <= 254;
    let session_id = req.session_id.as_deref().unwrap_or_default().trim();
    if session_id.len() < 8 || session_id.len() > 128 {
        return Err(ApiError::bad(
            "invalid_chat_session",
            "chat session is invalid",
        ));
    }
    let count:i32=sqlx::query_scalar("INSERT INTO checkout_agent_sessions(payment_id,session_id) VALUES($1,$2) ON CONFLICT(payment_id,session_id) DO UPDATE SET requests=CASE WHEN checkout_agent_sessions.window_started_at<now()-interval '1 minute' THEN 1 ELSE checkout_agent_sessions.requests+1 END,window_started_at=CASE WHEN checkout_agent_sessions.window_started_at<now()-interval '1 minute' THEN now() ELSE checkout_agent_sessions.window_started_at END RETURNING requests")
        .bind(payment.id.0).bind(session_id).fetch_one(state.store.pool()).await.map_err(db)?;
    if count>15{return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS,"chat_rate_limited","Wait a minute before sending another message."));}
    let latest_user_text = req
        .messages
        .iter()
        .rev()
        .find(|message| message.role.eq_ignore_ascii_case("user"))
        .map(|message| message.content.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(latest_user_text.as_str(),"hi"|"hello"|"hey"){
        return Ok(Json(json!({"reply":"Hi. I can help you pay this checkout, check its status, or investigate a transfer. What do you need help with?","status":"CHAT"})));
    }
    if latest_user_text.contains("how do i pay")||latest_user_text.contains("how to pay"){
        return Ok(Json(json!({"reply":format!("Send exactly {} {} on {} to the address shown on this checkout. Scan the QR code or copy the address into your wallet. The checkout updates automatically after the transfer is confirmed.",payment.expected_amount.to_decimal(payment.expected_asset.decimals),payment.expected_asset.symbol,payment.expected_chain.to_string().replace("custom:","").replace('_'," ")),"status":"CHAT"})));
    }
    if matches!(latest_user_text.as_str(),"status"|"payment status"|"check payment status"){
        return Ok(Json(json!({"reply":format!("This payment is currently {}. FlowPay updates the checkout when it detects and confirms your transfer.",payment.state.as_str().to_lowercase().replace('_'," ")),"status":"CHAT"})));
    }
    let greeting = latest_user_text
        .trim_matches(|character: char| !character.is_alphanumeric() && !character.is_whitespace());
    if matches!(
        greeting,
        "hi" | "hello" | "hey" | "hiya" | "howdy" | "hi there" | "hello there" | "hey there"
    ) {
        return Ok(Json(json!({
            "reply":"Hi. What would you like help with today?",
            "email":if email_verified { Some(&email) } else { None }
        })));
    }
    let user_conversation = req
        .messages
        .iter()
        .filter(|message| message.role.eq_ignore_ascii_case("user"))
        .map(|message| message.content.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let recovery_intent_from_user = [
        "wrong asset",
        "wrong token",
        "wrong network",
        "recover",
        "refund",
        "sent by mistake",
        "instead of",
    ]
    .iter()
    .any(|phrase| user_conversation.contains(phrase));
    let user_network = if user_conversation.contains("base") {
        Some("base sepolia".to_owned())
    } else if user_conversation.contains("ethereum sepolia")
        || user_conversation.contains("eth sepolia")
        || user_conversation.contains("sepolia")
    {
        Some("ethereum sepolia".to_owned())
    } else {
        None
    };
    let user_token = if user_conversation.split_whitespace().any(|word| {
        word.trim_matches(|character: char| !character.is_ascii_alphanumeric()) == "eth"
    }) {
        Some("ETH".to_owned())
    } else if user_conversation.contains("usdc") {
        Some("USDC".to_owned())
    } else if user_conversation.contains("usdt") {
        Some("USDT".to_owned())
    } else {
        None
    };
    let user_tx_hash = req
        .messages
        .iter()
        .filter(|message| message.role.eq_ignore_ascii_case("user"))
        .flat_map(|message| message.content.split_whitespace())
        .map(|word| {
            word.trim_matches(|character: char| !character.is_ascii_hexdigit() && character != 'x')
        })
        .find(|word| {
            word.len() == 66
                && word.starts_with("0x")
                && word[2..]
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
        })
        .map(str::to_owned);
    let user_amount = req
        .messages
        .iter()
        .filter(|message| message.role.eq_ignore_ascii_case("user"))
        .flat_map(|message| message.content.split_whitespace())
        .map(|word| {
            word.trim_matches(|character: char| !character.is_ascii_digit() && character != '.')
        })
        .find(|word| !word.is_empty() && word.parse::<f64>().is_ok_and(|value| value > 0.0))
        .map(str::to_owned);

    if recovery_intent_from_user
        && email_verified
        && user_network.is_some()
        && user_token.is_some()
        && user_amount.is_some()
        && user_tx_hash.is_some()
    {
        let network = user_network.as_deref().unwrap_or_default();
        let tx_hash = user_tx_hash.as_deref().unwrap_or_default();
        let chain_key = match network {
            "ethereum" | "eth" | "ethereum sepolia" | "eth sepolia" => {
                ChainKey::Custom("ethereum_sepolia".into())
            }
            "base" | "base sepolia" => ChainKey::Custom("base_sepolia".into()),
            "arbitrum" | "arb" | "arb sepolia" => ChainKey::Custom("arbitrum_sepolia".into()),
            _ => payment.expected_chain.clone(),
        };
        let chain = state.config.chains.get(&chain_key).ok_or_else(|| {
            ApiError::bad(
                "unsupported_claim_chain",
                "claimed network is not configured",
            )
        })?;
        if resolve_transaction_tool(&state, chain, tx_hash, &payment.checkout_address.value)
            .await?
            .is_some()
        {
            if let Some(existing_claim) = sqlx::query_scalar::<_, String>("SELECT public_id FROM claims WHERE payment_id=$1 AND lower(claimed_transaction_hash)=lower($2) ORDER BY created_at DESC LIMIT 1")
                .bind(payment.id.0).bind(tx_hash).fetch_optional(state.store.pool()).await.map_err(db)?
            {
                return Ok(Json(json!({"reply":format!("I called the internal transaction resolver and verified the {} {} transfer to this checkout. Recovery claim {existing_claim} is already under investigation.",user_amount.as_deref().unwrap_or_default(),user_token.as_deref().unwrap_or_default()),"claim_id":existing_claim,"status":"CLAIM_CREATED","email":email})));
            }
        }
    }

    if recovery_intent_from_user {
        let reply = if user_network.is_none() {
            Some(
                "I understand that this is a payment recovery. Which network did you send it on?"
                    .to_owned(),
            )
        } else if user_token.is_none() {
            Some(format!(
                "I have the network as {}. Which token did you send?",
                user_network.as_deref().unwrap_or_default()
            ))
        } else if user_amount.is_none() {
            Some(format!(
                "I have {} on {}. How much did you send?",
                user_token.as_deref().unwrap_or_default(),
                user_network.as_deref().unwrap_or_default()
            ))
        } else if user_tx_hash.is_none() {
            Some(format!(
                "I have {} {} on {}. What is the transaction hash?",
                user_amount.as_deref().unwrap_or_default(),
                user_token.as_deref().unwrap_or_default(),
                user_network.as_deref().unwrap_or_default()
            ))
        } else if !email_verified {
            Some("I have the transaction details. What email address should I use for investigation updates?".to_owned())
        } else {
            None
        };
        if let Some(reply) = reply {
            return Ok(Json(
                json!({"reply":reply,"email":if email_verified { Some(&email) } else { None }}),
            ));
        }
    }

    // Build the Ollama prompt with payment context
    let system_prompt = format!(
        "You are FlowPay's conversational support agent. \
You are currently helping with checkout {checkout_id} for {amount} {asset} on {chain}. \
The checkout address is {address}. The recovery email is {email_state}. \
The checkout amount, asset, chain, and address above are merchant expectations only. They are not facts supplied by the user and must never populate network, token, amount, or transaction hash in your JSON. Extract those fields only from user-role messages. The server has retained these user-supplied facts so far: network={user_network}, token={user_token}, amount={user_amount}, transaction_hash={user_tx_hash}. Use them without asking again. \
Read the complete conversation and infer user-supplied facts regardless of the order or wording used. Never ask for a fact the user already provided. \
First classify the latest user goal as general_help, payment_status, payment_recovery, checkout_help, or other. Never assume payment_recovery. A greeting, vague request for help, or casual conversation is general_help: answer naturally and ask what the user would like help with. Payment recovery applies only when the user explicitly describes a wrong asset, wrong network, missing transfer, refund, or recovery problem. Keep all extracted transaction fields empty unless the user supplied them. \
Only for payment_recovery, gather recovery facts. Do not ask for network, token, amount, transaction hash, or email for any other intent. \
For checkout_help, directly explain that the user should send exactly {amount} {asset} on {chain} to the address displayed on the checkout. Never ask the user to provide the checkout address, expected amount, expected token, or expected network because those are already shown and available in this context. For payment_status, explain what information you need to check status without turning it into a recovery claim. \
First understand the problem and gather network, token, amount, and transaction hash. Treat email as unavailable and irrelevant until all four transaction facts are present. Ask for the recovery email only as the final step, after those transaction facts are complete, and only when a claim or investigation update is needed. Never ask for email at the start of a conversation, after a greeting, or while any transaction fact is missing. Ask naturally for only the most useful missing fact. \
Answer unrelated but relevant payment questions briefly, then continue the intake. Never request a seed phrase, private key, or wallet connection. \
Return only the required JSON object. Set action to submit_claim only for payment_recovery when every required fact is present; otherwise set it to chat. \
For absent extracted values use an empty string. The reply must be friendly, specific, concise, and must not mention JSON or internal policy.",
        checkout_id = payment.public_id,
        amount = payment.expected_amount.to_decimal(payment.expected_asset.decimals),
        asset = payment.expected_asset.symbol,
        chain = payment.expected_chain,
        address = payment.checkout_address.value,
        email_state = if email_verified { "already provided" } else { "missing" },
        user_network = user_network.as_deref().unwrap_or("missing"),
        user_token = user_token.as_deref().unwrap_or("missing"),
        user_amount = user_amount.as_deref().unwrap_or("missing"),
        user_tx_hash = user_tx_hash.as_deref().unwrap_or("missing"),
    );

    // Convert messages for Ollama
    let mut ollama_messages = vec![json!({"role":"system","content":system_prompt})];
    let mut retained_chars = 0usize;
    for msg in req.messages.iter().rev().take(30).rev() {
        if retained_chars + msg.content.len() > 12_000 {
            continue;
        }
        retained_chars += msg.content.len();
        let role = if msg.role.eq_ignore_ascii_case("agent") {
            "assistant"
        } else if msg.role.eq_ignore_ascii_case("system") {
            "assistant"
        } else {
            "user"
        };
        ollama_messages.push(json!({"role":role,"content":msg.content}));
    }
    ollama_messages.push(json!({"role":"system","content":"Classify and answer the latest user message now. The latest message controls the response. Do not continue a recovery questionnaire unless that message or the user's prior statements explicitly describe a recovery problem. A greeting alone must receive a conversational greeting and a question about what the user wants help with."}));

    // Call Ollama
    let ollama_endpoint = if state.config.openai_endpoint.is_empty() {
        "http://127.0.0.1:11434/api/chat".to_owned()
    } else if state.config.openai_endpoint.ends_with("/api/chat") {
        state.config.openai_endpoint.clone()
    } else {
        format!(
            "{}/api/chat",
            state.config.openai_endpoint.trim_end_matches('/')
        )
    };
    let ollama_model = &state.config.openai_model;
    let response = state
        .http
        .post(ollama_endpoint)
        .timeout(StdDuration::from_secs(12))
        .json(&json!({
            "model": ollama_model,
            "messages": ollama_messages,
            "stream": false,
            "format": {
                "type":"object",
                "properties":{
                    "intent":{"type":"string","enum":["general_help","payment_status","payment_recovery","checkout_help","other"]},
                    "action":{"type":"string","enum":["chat","submit_claim"]},
                    "reply":{"type":"string"},
                    "network":{"type":"string"},
                    "token":{"type":"string"},
                    "amount":{"type":"string"},
                    "tx_hash":{"type":"string"}
                },
                "required":["intent","action","reply","network","token","amount","tx_hash"]
            },
            "options": {"temperature": 0.2,"num_ctx":4096,"num_predict":100,"repeat_penalty":1.1}
        }))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let payload: Value = resp.json().await.map_err(internal)?;
            let model_content = payload
                .pointer("/message/content")
                .and_then(Value::as_str)
                .unwrap_or("I'm here to help. Could you tell me more about your issue?");
            let parsed_model = serde_json::from_str::<Value>(model_content).unwrap_or_else(|_| {
                let trimmed = model_content.trim_start();
                let safe_reply = if trimmed.starts_with('{')
                    || trimmed.starts_with('[')
                    || model_content.contains("\"action\"")
                    || model_content.contains("\"intent\"")
                {
                    ""
                } else {
                    model_content
                };
                json!({"intent":"other","action":"chat","reply":safe_reply,"network":"","token":"","amount":"","tx_hash":""})
            });
            let content = parsed_model.to_string();

            // Check if the agent decided to submit a claim
            if serde_json::from_str::<Value>(&content).is_ok() {
                let complete_claim = user_network.is_some()
                    && user_token.is_some()
                    && user_amount.is_some()
                    && user_tx_hash.is_some();
                if email_verified && complete_claim && recovery_intent_from_user {
                    let network = user_network.as_deref().unwrap_or("");
                    let token = user_token.as_deref().unwrap_or("");
                    let amount = user_amount.as_deref().unwrap_or("");
                    let tx_hash = user_tx_hash.as_deref().unwrap_or("");
                    // Map network string to ChainKey
                    let chain_key = match network.to_lowercase().as_str() {
                        "ethereum" | "eth" | "ethereum sepolia" | "eth sepolia" => {
                            ChainKey::Custom("ethereum_sepolia".into())
                        }
                        "base" | "base sepolia" => ChainKey::Custom("base_sepolia".into()),
                        "arbitrum" | "arb" | "arb sepolia" => {
                            ChainKey::Custom("arbitrum_sepolia".into())
                        }
                        _ => payment.expected_chain.clone(),
                    };

                    let chain = state.config.chains.get(&chain_key).ok_or_else(|| {
                        ApiError::bad(
                            "unsupported_claim_chain",
                            "claimed network is not configured",
                        )
                    })?;
                    let Some(resolved) = resolve_transaction_tool(
                        &state,
                        chain,
                        tx_hash,
                        &payment.checkout_address.value,
                    )
                    .await?
                    else {
                        return Ok(Json(json!({
                            "reply":format!("I traced the full transaction execution, but found no value transfer to this checkout address {}. Check that the hash belongs to this payment.",payment.checkout_address.value),
                            "status":"PAYMENT_ADDRESS_MISMATCH"
                        })));
                    };
                    let originating_wallet = resolved.effective_sender.clone();
                    if let Some(existing_claim) = sqlx::query_scalar::<_, String>("SELECT public_id FROM claims WHERE payment_id=$1 AND lower(claimed_transaction_hash)=lower($2) ORDER BY created_at DESC LIMIT 1")
                        .bind(payment.id.0).bind(tx_hash).fetch_optional(state.store.pool()).await.map_err(db)?
                    {
                        return Ok(Json(json!({"reply":format!("I resolved the transaction and verified its transfer to this checkout. Recovery claim {existing_claim} is already under investigation."),"claim_id":existing_claim,"status":"CLAIM_CREATED","email":email})));
                    }

                    // Create the claim via existing infrastructure
                    let claim_id = ClaimId::new();
                    let public_id = format!("clm_{}", claim_id.0.simple());
                    let claim = StoredClaim {
                        id: claim_id,
                        public_id: public_id.clone(),
                        merchant_id: merchant,
                        payment_id: payment.id,
                        state: ClaimState::Investigating,
                        expected_chain: payment.expected_chain.clone(),
                        claimed_chain: Some(chain_key.clone()),
                        expected_asset: payment.expected_asset.symbol.clone(),
                        claimed_asset: Some(token.to_owned()),
                        transaction_hash: Some(tx_hash.to_owned()),
                        originating_wallet: Some(originating_wallet.clone()),
                        recovery_destination: originating_wallet.clone(),
                        explanation: format!(
                            "Wrong asset: sent {} {} (expected {} {}) on {}",
                            amount,
                            token,
                            payment
                                .expected_amount
                                .to_decimal(payment.expected_asset.decimals),
                            payment.expected_asset.symbol,
                            network
                        ),
                    };
                    state.store.create_claim(&claim).await.map_err(db)?;
                    state
                        .store
                        .set_payment_state(
                            payment.id,
                            PaymentState::ClaimPending,
                            "agent_intake_submitted",
                            Some(&chain_key),
                            Some(tx_hash),
                        )
                        .await
                        .map_err(db)?;
                    sqlx::query("INSERT INTO agent_chat_intakes(id,payment_id,claim_id,session_id,email,gathered_data) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(payment_id,session_id) DO UPDATE SET claim_id=EXCLUDED.claim_id,email=EXCLUDED.email,gathered_data=EXCLUDED.gathered_data,updated_at=now()")
                        .bind(Uuid::now_v7()).bind(payment.id.0).bind(claim_id.0).bind(session_id).bind(&email)
                        .bind(json!({"network":network,"token":token,"amount":amount,"transaction_hash":tx_hash,"originating_wallet":originating_wallet,"resolution":{"tool":"resolve_transaction","effective_destination":resolved.effective_destination,"value_hex":resolved.value_hex,"used_internal_call":resolved.used_internal_call}}))
                        .execute(state.store.pool()).await.map_err(db)?;

                    return Ok(Json(json!({
                        "reply":format!("I resolved the transaction and verified the {} {} transfer to this checkout{}. Recovery claim {public_id} is now under investigation.",amount,token,if resolved.used_internal_call { " through an internal smart-account call" } else { "" }),
                        "claim_id": public_id,
                        "status": "CLAIM_CREATED",
                        "email":email
                    })));
                }
            }

            let response_value = serde_json::from_str::<Value>(&content).unwrap_or(Value::Null);
            let recovery_intent = recovery_intent_from_user;
            let mut reply = response_value
                .get("reply")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    if recovery_intent {
                        "Tell me what happened with this payment and I will investigate it."
                            .to_owned()
                    } else {
                        "What would you like help with today? I can explain this checkout, check a payment, or help with a recovery.".to_owned()
                    }
                });
            let missing_transaction_question = if !recovery_intent {
                None
            } else if user_network.is_none() {
                Some("I understand that you sent a different asset to this checkout. Which network did you send it on?".to_owned())
            } else if user_token.is_none() {
                Some(format!(
                    "Thanks, I have the network as {}. Which token did you send?",
                    user_network.as_deref().unwrap_or_default()
                ))
            } else if user_amount.is_none() {
                Some(format!(
                    "Thanks, I have {} on {}. How much did you send?",
                    user_token.as_deref().unwrap_or_default(),
                    user_network.as_deref().unwrap_or_default()
                ))
            } else if user_tx_hash.is_none() {
                Some(format!(
                    "I have {} {} on {}. What is the transaction hash?",
                    user_amount.as_deref().unwrap_or_default(),
                    user_token.as_deref().unwrap_or_default(),
                    user_network.as_deref().unwrap_or_default()
                ))
            } else {
                None
            };
            if let Some(question) = missing_transaction_question.as_deref() {
                reply = question.to_owned();
            } else if recovery_intent && !email_verified {
                let asks_for_email = reply.to_ascii_lowercase().contains("email");
                if !asks_for_email {
                    reply.push(' ');
                    reply.push_str("What email address should I use for investigation updates?");
                }
            }
            Ok(Json(
                json!({"reply": reply,"email":if email_verified { Some(&email) } else { None }}),
            ))
        }
        _ => {
            // Ollama unavailable — fall back to simple guided flow
            let last_user = req
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map_or("", |m| m.content.as_str());
            let lower = last_user.to_lowercase();
            let normalized = lower.trim().trim_matches(|c: char| !c.is_alphanumeric());
            let is_greeting = matches!(normalized, "hi" | "hello" | "hey" | "hiya" | "howdy");
            let reply = if is_greeting {
                "Hi. What would you like help with today?".to_string()
            } else if !recovery_intent_from_user {
                "I can help with this checkout, payment status, or a payment recovery. What would you like me to look into?".to_string()
            } else if user_network.is_none() {
                "I understand that this is a recovery. Which network did you send it on?"
                    .to_string()
            } else if user_token.is_none() {
                format!(
                    "I have the network as {}. Which token did you send?",
                    user_network.as_deref().unwrap_or_default()
                )
            } else if user_amount.is_none() {
                format!(
                    "I have {} on {}. How much did you send?",
                    user_token.as_deref().unwrap_or_default(),
                    user_network.as_deref().unwrap_or_default()
                )
            } else if user_tx_hash.is_none() {
                format!(
                    "I have {} {} on {}. What is the transaction hash?",
                    user_amount.as_deref().unwrap_or_default(),
                    user_token.as_deref().unwrap_or_default(),
                    user_network.as_deref().unwrap_or_default()
                )
            } else if !email_verified {
                "I have the transaction details. What email address should I use for investigation updates?".to_string()
            } else {
                "I have all the recovery details. The investigation service is taking longer than expected; please send continue to retry submission.".to_string()
            };
            Ok(Json(json!({"reply": reply})))
        }
    }
}

#[cfg(test)]
mod alchemy_checkout_sync_tests {
    use super::*;
    use crate::config::Config;
    use std::collections::HashMap;

    #[test]
    fn nested_smart_account_native_transfer_is_found() {
        let trace = json!({
            "to":"0xentrypoint",
            "value":"0x0",
            "calls":[{"from":"0xsmartaccount","to":"0xCheckout","value":"0x5af3107a4000"}]
        });
        assert_eq!(
            find_native_transfer_in_trace(&trace, "0xcheckout"),
            Some(("0xsmartaccount".to_owned(), "0x5af3107a4000".to_owned()))
        );
    }

    #[test]
    fn zero_value_or_wrong_destination_is_not_a_transfer() {
        let trace = json!({
            "to":"0xentrypoint",
            "value":"0x0",
            "calls":[{"from":"0xsmartaccount","to":"0xother","value":"0x5af3107a4000"}]
        });
        assert_eq!(find_native_transfer_in_trace(&trace, "0xcheckout"), None);
    }

    fn test_config() -> Config {
        let mut chains = HashMap::new();
        chains.insert(
            ChainKey::Custom("base_sepolia".into()),
            crate::config::ChainConfig {
                chain: ChainKey::Custom("base_sepolia".into()),
                rpc_url: "https://base-sepolia-rpc.publicnode.com".into(),
                numeric_chain_id: 84532,
                factory_address: "0x351E7e39456c21f6d2aF3fDf3bcd391E92775cb5".into(),
            },
        );
        chains.insert(
            ChainKey::Custom("bsc_testnet".into()),
            crate::config::ChainConfig {
                chain: ChainKey::Custom("bsc_testnet".into()),
                rpc_url: "https://bsc-testnet-rpc.publicnode.com".into(),
                numeric_chain_id: 97,
                factory_address: "0x351E7e39456c21f6d2aF3fDf3bcd391E92775cb5".into(),
            },
        );
        Config {
            bind: "0.0.0.0:8080".into(),
            database_url: "postgres://flowpay:flowpay@localhost:5432/flowpay".into(),
            environment: "production".into(),
            checkout_base_url: "https://checkout.example.test".into(),
            api_key_pepper: "pepper".into(),
            proxy_creation_code_hash:
                "0x7fec5ea1a8a7c531e89efcb2bb71a816cf43f4ee27803cba624500c6634919d8".into(),
            factory_runtime_code_hash: Some(
                "0x3081d39267aa8c34d6e4c87c5662cf873e94c7e9a8d136810cf6c54e82262427".into(),
            ),
            operator_address: "0xe6E2aB64586E82aB45B27FCC7ED286850269e4Eb".into(),
            operator_private_key: None,
            faucet_address: None,
            evidence_dir: "./runtime/evidence".into(),
            webhook_encryption_key: vec![0_u8; 32],
            chains,
            agent_mode: "model".into(),
            model_provider: "ollama".into(),
            openai_api_key: None,
            openai_model: "qwen2.5-coder:7b".into(),
            openai_endpoint: "http://127.0.0.1:11434/api/chat".into(),
            agent_max_steps: 12,
            agent_retry_budget: 3,
            rabbitmq_url: "amqp://guest:guest@127.0.0.1:5672/%2f".into(),
            provider_webhook_secret: None,
            provider_webhook_secrets: vec![],
            provider_webhook_path: "/v1/providers/alchemy/webhook".into(),
            provider_webhook_url: None,
            alchemy_api_key: None,
            alchemy_networks: vec![],
            alchemy_notify_auth_token: None,
            alchemy_webhook_ids: HashMap::new(),
            alchemy_notify_endpoint: "https://dashboard.alchemy.com/api/update-webhook-addresses"
                .into(),
            flutterwave_base_url: "https://api.flutterwave.com/v3".into(),
            flutterwave_secret_key: None,
            flutterwave_secret_hash: None,
            flutterwave_customer_email: "info@landaa.xyz".into(),
            flutterwave_customer_first_name: "Lander".into(),
            flutterwave_customer_last_name: "Global LTD".into(),
            ngn_platform_fee_atomic: 5_000,
            admin_key: None,
            flutterwave_fee_bps: 200,
            flutterwave_fee_vat_bps: 750,
            formance_base_url: "http://127.0.0.1:8081".into(),
            formance_ledger: "flowpay".into(),
            formance_token: None,
        }
    }

    /// Local mode must not require any Alchemy configuration.
    #[test]
    fn local_mode_is_exempt_from_alchemy_requirements() {
        let mut config = test_config();
        config.environment = "local".into();
        config.alchemy_notify_auth_token = None;
        config.alchemy_webhook_ids = HashMap::new();

        let monitored = alchemy_webhook_networks(&config);
        assert!(monitored.is_empty());
    }

    /// Only chains with a configured webhook ID are monitored.
    #[test]
    fn webhook_networks_require_existing_webhook_ids() {
        let mut config = test_config();
        config.environment = "production".into();
        config.alchemy_networks = vec!["base-sepolia".into(), "bsc-testnet".into()];
        config.alchemy_webhook_ids = HashMap::new();

        let monitored = alchemy_webhook_networks(&config);
        assert!(monitored.is_empty());
    }

    /// Configured webhook IDs must line up with `ALCHEMY_NETWORKS`.
    #[test]
    fn webhook_networks_only_include_configured_chains() {
        let mut config = test_config();
        config.environment = "production".into();
        config.alchemy_networks = vec!["base-sepolia".into(), "bsc-testnet".into()];
        config
            .alchemy_webhook_ids
            .insert(ChainKey::Custom("base_sepolia".into()), "wh_base".into());
        config
            .alchemy_webhook_ids
            .insert(ChainKey::Custom("bsc_testnet".into()), "wh_bsc".into());

        let monitored = alchemy_webhook_networks(&config);
        let pairs: Vec<_> = monitored
            .into_iter()
            .map(|(network, chain)| (network, chain.to_string()))
            .collect();

        // ALCHEMY_NETWORKS names are normalized before matching webhook IDs.
        assert!(pairs.contains(&("BASE_SEPOLIA".into(), "base_sepolia".into())));
        assert!(pairs.contains(&("BNB_TESTNET".into(), "bsc_testnet".into())));
    }

    /// `ALCHEMY_NETWORKS` without webhook IDs must not pretend a webhook exists.
    #[test]
    fn mismatched_alchemy_networks_fails_fast() {
        let mut config = test_config();
        config.environment = "production".into();
        config.alchemy_networks = vec!["base-sepolia".into()];
        config.alchemy_webhook_ids = HashMap::new();

        let monitored = alchemy_webhook_networks(&config);
        assert!(monitored.is_empty());
    }
}
