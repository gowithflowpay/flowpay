use crate::{
    error::ApiError,
    routes::{db, load_merchant},
    state::AppState,
};
use axum::{extract::State, http::StatusCode, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ChallengeRequest {
    public_key: String,
    purpose: String,
    profile: Option<Profile>,
}

#[derive(Deserialize, serde::Serialize)]
struct Profile {
    business_name: String,
    email: String,
    settlement_address: String,
    #[serde(default)]
    contact_name: String,
}

fn invalid() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "device_auth_failed",
        "Device verification failed. Start again with the original device key.",
    )
}

fn key(value: &str) -> Result<VerifyingKey, ApiError> {
    let raw: [u8; 32] = B64
        .decode(value)
        .map_err(|_| invalid())?
        .try_into()
        .map_err(|_| invalid())?;
    let key = VerifyingKey::from_bytes(&raw).map_err(|_| invalid())?;
    if key.is_weak() || B64.encode(raw) != value {
        return Err(invalid());
    }
    Ok(key)
}

pub async fn challenge(
    State(state): State<AppState>,
    Json(mut req): Json<ChallengeRequest>,
) -> Result<Json<Value>, ApiError> {
    key(&req.public_key)?;
    if !matches!(req.purpose.as_str(), "REGISTER" | "LOGIN") {
        return Err(ApiError::bad(
            "invalid_purpose",
            "Invalid authentication purpose",
        ));
    }
    if req.purpose == "REGISTER" {
        let profile = req.profile.as_mut().ok_or_else(|| {
            ApiError::bad(
                "profile_required",
                "Business, recovery email and settlement wallet are required",
            )
        })?;
        profile.business_name = profile.business_name.trim().to_owned();
        profile.email = profile.email.trim().to_lowercase();
        let email_parts: Vec<_> = profile.email.split('@').collect();
        if profile.business_name.is_empty()
            || profile.business_name.len() > 120
            || profile.contact_name.len() > 120
            || profile.email.len() > 254
            || profile.email.chars().any(char::is_whitespace)
            || email_parts.len() != 2
            || email_parts[0].is_empty()
            || !email_parts[1].contains('.')
            || profile.settlement_address.len() != 42
            || !profile.settlement_address.starts_with("0x")
            || !profile.settlement_address[2..]
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            || profile.settlement_address[2..].bytes().all(|b| b == b'0')
        {
            return Err(ApiError::bad(
                "invalid_profile",
                "Enter a valid business name, recovery email and EVM settlement wallet",
            ));
        }
    } else {
        req.profile = None;
    }
    let id = Uuid::now_v7();
    let expires = OffsetDateTime::now_utc() + Duration::minutes(2);
    let profile = req
        .profile
        .map(serde_json::to_value)
        .transpose()
        .map_err(db)?;
    // Random nonce and the persisted profile bind the proof to this exact operation.
    let message = format!(
        "FlowPay CLI authentication\n{}\n{}\n{}\n{}",
        req.purpose,
        id,
        crate::auth::new_session_token(),
        profile.as_ref().map(Value::to_string).unwrap_or_default()
    );
    sqlx::query("DELETE FROM cli_key_challenges WHERE expires_at<=now()")
        .execute(state.store.pool())
        .await
        .map_err(db)?;
    let mut tx = state.store.pool().begin().await.map_err(db)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&req.public_key)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM cli_key_challenges WHERE public_key=$1 AND created_at>now()-interval '2 minutes'")
        .bind(&req.public_key).fetch_one(&mut *tx).await.map_err(db)?;
    if count >= 5 {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "device_rate_limited",
            "Wait two minutes before trying again",
        ));
    }
    sqlx::query("INSERT INTO cli_key_challenges(id,public_key,purpose,message,profile,expires_at) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(req.public_key).bind(req.purpose).bind(&message).bind(profile).bind(expires).execute(&mut *tx).await.map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(
        json!({"id":id,"message":message,"expires_at":expires.unix_timestamp()*1000}),
    ))
}

#[derive(Deserialize)]
pub struct FinishRequest {
    id: Uuid,
    signature: String,
}

pub async fn finish(
    State(state): State<AppState>,
    Json(req): Json<FinishRequest>,
) -> Result<Json<Value>, ApiError> {
    // DELETE is committed before verification, so failed proofs and replay attempts
    // cannot reuse a challenge, even across concurrent requests.
    let row = sqlx::query("DELETE FROM cli_key_challenges WHERE id=$1 AND expires_at>now() RETURNING public_key,purpose,message,profile")
        .bind(req.id).fetch_optional(state.store.pool()).await.map_err(db)?.ok_or_else(invalid)?;
    let public: String = row.get("public_key");
    let signature = Signature::from_slice(&B64.decode(req.signature).map_err(|_| invalid())?)
        .map_err(|_| invalid())?;
    let message: String = row.get("message");
    key(&public)?
        .verify_strict(message.as_bytes(), &signature)
        .map_err(|_| invalid())?;
    let mut tx = state.store.pool().begin().await.map_err(db)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&public)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let existing: Option<Uuid> = sqlx::query_scalar("SELECT k.merchant_id FROM merchant_cli_keys k JOIN merchants m ON m.id=k.merchant_id WHERE k.public_key=$1 AND k.revoked_at IS NULL AND m.status='ACTIVE'")
        .bind(&public).fetch_optional(&mut *tx).await.map_err(db)?;
    let purpose: String = row.get("purpose");
    let merchant = if let Some(merchant) = existing {
        merchant
    } else {
        if purpose != "REGISTER" {
            return Err(invalid());
        }
        let registered: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM merchant_cli_keys WHERE public_key=$1)",
        )
        .bind(&public)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if registered {
            return Err(invalid());
        }
        let profile: Profile =
            serde_json::from_value(row.get::<Value, _>("profile")).map_err(db)?;
        let id = Uuid::now_v7();
        let inserted = sqlx::query("INSERT INTO merchants(id,public_id,name,status,email,contact_name,evm_settlement_address,onboarding_completed_at) VALUES($1,$2,$3,'ACTIVE',$4,$5,$6,now()) ON CONFLICT DO NOTHING")
            .bind(id).bind(format!("mer_{}", &id.simple().to_string()[..20])).bind(profile.business_name).bind(profile.email)
            .bind(profile.contact_name).bind(profile.settlement_address).execute(&mut *tx).await.map_err(db)?;
        if inserted.rows_affected() != 1 {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "account_exists",
                "This recovery email is already registered. Sign in with the original device key.",
            ));
        }
        sqlx::query("INSERT INTO merchant_cli_keys(public_key,merchant_id) VALUES($1,$2)")
            .bind(&public)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        id
    };
    let token = crate::auth::new_session_token();
    let expires =
        OffsetDateTime::now_utc() + Duration::hours(state.config.auth_session_ttl_hours.max(1));
    sqlx::query("INSERT INTO merchant_sessions(id,merchant_id,token_hash,user_agent,expires_at) VALUES($1,$2,$3,'FlowPay CLI device key',$4)")
        .bind(Uuid::now_v7()).bind(merchant).bind(crate::auth::hash_secret(&token)).bind(expires).execute(&mut *tx).await.map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(
        json!({"session_token":token,"expires_at":expires.unix_timestamp()*1000,"merchant":load_merchant(&state,merchant).await?}),
    ))
}
