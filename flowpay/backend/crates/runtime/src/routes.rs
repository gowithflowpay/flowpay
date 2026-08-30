use crate::{error::ApiError, state::AppState};
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
    Payment, PaymentId,
};
use flowpay_messaging::{enqueue_command_tx, enqueue_domain_event_tx};
use flowpay_payments::derive_checkout_salt;
use flowpay_persistence::{StoreError, StoredClaim};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{path::PathBuf, str::FromStr};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/providers/alchemy/webhook", post(alchemy_webhook))
        .route("/v1/payments", get(list_payments).post(create_payment))
        .route("/v1/payments/{id}", get(get_payment))
        .route("/v1/payments/{id}/cancel", post(cancel_payment))
        .route("/v1/payments/{id}/deposits", get(get_deposits))
        .route("/v1/claims", get(list_claims).post(create_claim))
        .route("/v1/claims/{id}", get(get_claim))
        .route("/v1/claims/{id}/evidence", post(add_evidence))
        .route("/v1/claims/{id}/authorize", post(authorize_claim))
        .route("/v1/claims/{id}/fund", post(fund_claim))
        .route("/v1/claims/{id}/approve", post(approve_claim))
        .route("/v1/webhooks", get(list_webhooks).post(create_webhook))
        .route("/v1/webhooks/test", post(test_webhook))
        .route("/v1/api-keys", get(list_api_keys).post(create_api_key))
        .route("/v1/api-keys/{id}/revoke", post(revoke_api_key))
        .route("/v1/logs", get(list_logs))
        .route("/v1/overview", get(get_overview))
        .route("/v1/merchant/overview", get(get_overview))
        .with_state(state)
}

async fn alchemy_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<StatusCode, ApiError> {
    let secret = state
        .config
        .provider_webhook_secret
        .as_deref()
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                "provider_webhook_not_configured",
                "provider webhook secret is not configured",
            )
        })?;
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
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(internal)?;
    mac.update(body.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    let supplied = signature.trim_start_matches("0x");
    if expected.len() != supplied.len()
        || !expected
            .as_bytes()
            .iter()
            .zip(supplied.as_bytes())
            .fold(0_u8, |diff, (left, right)| diff | (left ^ right))
            .eq(&0)
    {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_provider_signature",
            "invalid Alchemy signature",
        ));
    }
    let payload: Value = serde_json::from_str(&body)
        .map_err(|_| ApiError::bad("invalid_provider_payload", "provider payload must be JSON"))?;
    let event_id = payload
        .get("webhookId")
        .or_else(|| payload.get("id"))
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
}

async fn create_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreatePaymentRequest>,
) -> Result<(StatusCode, Json<PaymentResponse>), ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let idem = idempotency_key(&headers)?;
    let request_bytes = serde_json::to_vec(&req).map_err(internal)?;
    let request_hash = hex::encode(Sha256::digest(&request_bytes));
    if let Some(existing) =
        load_idempotent_response(&state, merchant, "POST:/v1/payments", &idem, &request_hash)
            .await?
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
    let expires = OffsetDateTime::now_utc()
        + Duration::seconds(req.expires_in_seconds.unwrap_or(1800).clamp(60, 86_400));
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
        .create_payment(&payment, salt, &runtime.factory, "EVM_CREATE3_V1")
        .await
        .map_err(db)?;
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

async fn get_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let p = state
        .store
        .get_payment(merchant, &id)
        .await
        .map_err(map_store)?;
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
async fn cancel_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
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
async fn get_deposits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
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
        load_idempotent_response(&state, merchant, "POST:/v1/claims", &idem, &request_hash).await?
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
    let _ = state
        .store
        .set_payment_state(
            payment.id,
            flowpay_domain::PaymentState::ClaimPending,
            "claim_created",
            claimed_chain.as_ref(),
            req.transaction_hash.as_deref(),
        )
        .await;
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
    let funds_present = latest_tool_output("get_token_balance")
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
        let _ = state
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
            .await;
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
            || host.starts_with("169.254."))
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

async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<MerchantId, ApiError> {
    let key = headers
        .get("x-flowpay-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "missing_api_key",
                "x-flowpay-api-key is required",
            )
        })?;
    let prefix = key.split('.').next().unwrap_or(key);
    let record = state.store.api_key_by_prefix(prefix).await.map_err(|_| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "invalid API key",
        )
    })?;
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
    if computed != record.secret_hash {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "invalid API key",
        ));
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
async fn load_idempotent_response(
    state: &AppState,
    merchant: MerchantId,
    scope: &str,
    key: &str,
    request_hash: &str,
) -> Result<Option<Value>, ApiError> {
    let row=sqlx::query("SELECT request_hash,response_body FROM idempotency_keys WHERE merchant_id=$1 AND api_scope=$2 AND idempotency_key=$3 AND expires_at>now()").bind(merchant.0).bind(scope).bind(key).fetch_optional(state.store.pool()).await.map_err(db)?;
    if let Some(row) = row {
        let existing: String = row.try_get("request_hash").map_err(internal)?;
        if existing != request_hash {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "idempotency_conflict",
                "same Idempotency-Key was used with a different request",
            ));
        }
        return Ok(row.try_get("response_body").map_err(internal)?);
    }
    Ok(None)
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
    json!({"id":p.public_id,"address":p.checkout_address.value,"amount":p.expected_amount.to_decimal(p.expected_asset.decimals),"amount_atomic":p.expected_amount,"asset":p.expected_asset.symbol,"chain":p.expected_chain,"status":p.state.as_str(),"expires_at":p.expires_at.to_string(),"reference":p.reference,"merchant_name":merchant_name,"checkout_url":format!("{}/pay/{}",checkout_base_url.trim_end_matches('/'),p.public_id)})
}
fn parse_overpayment(value: Option<&str>) -> Result<OverpaymentPolicy, ApiError> {
    match value.unwrap_or("ACCEPT_AND_RECORD") {
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
fn map_store(e: StoreError) -> ApiError {
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
fn db<E: std::fmt::Display>(e: E) -> ApiError {
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
    let mut balances = Vec::new();
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
        match runtime.adapter.token_balance(&token,&settlement).await {
            Ok(amount)=>balances.push(json!({"chain":chain,"symbol":symbol,"contract":token,"decimals":decimals,"amount_atomic":amount.to_string(),"amount":amount.to_decimal(decimals as u8)})),
            Err(error)=>balances.push(json!({"chain":chain,"symbol":symbol,"contract":token,"decimals":decimals,"error":error.to_string()})),
        }
    }
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
    let merchant = authenticate(&state, &headers).await?;
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
}
async fn list_api_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let rows=sqlx::query("SELECT id,label,public_prefix,created_at,last_used_at,revoked_at FROM api_keys WHERE merchant_id=$1 ORDER BY created_at DESC").bind(merchant.0).fetch_all(state.store.pool()).await.map_err(db)?;
    Ok(Json(
        json!({"data":rows.iter().map(|r|json!({"id":format!("key_{}",r.try_get::<Uuid,_>("id").unwrap_or_default().simple()),"name":r.try_get::<String,_>("label").unwrap_or_default(),"prefix":r.try_get::<String,_>("public_prefix").unwrap_or_default(),"last_used_at":r.try_get::<Option<OffsetDateTime>,_>("last_used_at").ok().flatten().map(|v|v.to_string()),"revoked":r.try_get::<Option<OffsetDateTime>,_>("revoked_at").ok().flatten().is_some()})).collect::<Vec<_>>() }),
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
    let prefix = format!(
        "fp_{}_{}",
        if state.config.environment == "local" {
            "test"
        } else {
            "live"
        },
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
            json!({"id":format!("key_{}",id.simple()),"api_key":full,"warning":"This secret is shown once."}),
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
