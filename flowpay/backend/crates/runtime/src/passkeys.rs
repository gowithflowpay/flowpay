use crate::{auth::hash_secret, error::ApiError, routes::{authenticate_dashboard, create_session, db, load_merchant}, state::AppState};
use axum::{extract::State, http::{HeaderMap, StatusCode}, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;
use webauthn_rs::prelude::*;

fn webauthn() -> Result<Webauthn, ApiError> {
    let value = std::env::var("FLOWPAY_WEBAUTHN_ORIGIN")
        .or_else(|_| std::env::var("FLOWPAY_MERCHANT_BASE_URL"))
        .unwrap_or_else(|_| "https://pixuno.xyz".into());
    let origin = url::Url::parse(&value).map_err(internal)?;
    let rp_id = origin.host_str().ok_or_else(|| internal("passkey origin has no hostname"))?;
    WebauthnBuilder::new(rp_id, &origin).map_err(internal)?
        .rp_name("FlowPay").timeout(std::time::Duration::from_secs(300)).build().map_err(internal)
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    tracing::error!(%error, "passkey service error");
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "passkey_service_error", "Passkey setup is unavailable. Please try again.")
}

fn ceremony_error(_: impl std::fmt::Display) -> ApiError {
    ApiError::bad("passkey_verification_failed", "Passkey verification failed. Start again and approve the request on your device.")
}

fn session_hash(headers: &HeaderMap) -> Result<String, ApiError> {
    let token = headers.get("authorization").and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ")).filter(|v| !v.is_empty())
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED,"authentication_required","Sign in before adding a passkey."))?;
    Ok(hash_secret(token))
}

pub(crate) async fn require_registered(state: &AppState, merchant: Uuid) -> Result<(), ApiError> {
    let registered: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM merchant_passkeys WHERE merchant_id=$1)")
        .bind(merchant).fetch_one(state.store.pool()).await.map_err(db)?;
    if !registered { return Err(ApiError::bad("passkey_required", "Create your passkey before completing browser onboarding.")); }
    Ok(())
}

pub async fn status(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let merchant = authenticate_dashboard(&state,&headers).await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM merchant_passkeys WHERE merchant_id=$1")
        .bind(merchant.0).fetch_one(state.store.pool()).await.map_err(db)?;
    Ok(Json(json!({"registered":count>0,"count":count})))
}

async fn save_challenge<T: Serialize>(state: &AppState, merchant: Uuid, kind: &str, session: Option<String>, value: &T) -> Result<Uuid,ApiError> {
    sqlx::query("DELETE FROM merchant_passkey_challenges WHERE expires_at<=now()")
        .execute(state.store.pool()).await.map_err(db)?;
    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM merchant_passkey_challenges WHERE merchant_id=$1 AND expires_at>now()")
        .bind(merchant).fetch_one(state.store.pool()).await.map_err(db)?;
    if active>=10 { return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS,"passkey_rate_limited","Wait a few minutes before starting another passkey request.")); }
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO merchant_passkey_challenges(id,merchant_id,kind,session_hash,state,expires_at) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(merchant).bind(kind).bind(session).bind(serde_json::to_value(value).map_err(internal)?)
        .bind(OffsetDateTime::now_utc()+Duration::minutes(5))
        .execute(state.store.pool()).await.map_err(db)?;
    Ok(id)
}

pub async fn register_start(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>,ApiError> {
    let merchant = authenticate_dashboard(&state,&headers).await?;
    let account = load_merchant(&state,merchant.0).await?;
    if account["email_verified"]!=true {return Err(ApiError::bad("email_not_verified","Verify your email before creating a passkey."));}
    let rows = sqlx::query_scalar::<_,Value>("SELECT passkey FROM merchant_passkeys WHERE merchant_id=$1")
        .bind(merchant.0).fetch_all(state.store.pool()).await.map_err(db)?;
    let keys = rows.into_iter().map(serde_json::from_value::<Passkey>).collect::<Result<Vec<_>,_>>().map_err(internal)?;
    let ids = keys.iter().map(|key|key.cred_id().clone()).collect();
    let (options, registration) = webauthn()?.start_passkey_registration(merchant.0,
        account["email"].as_str().unwrap_or("FlowPay merchant"), account["business_name"].as_str().unwrap_or("FlowPay merchant"),Some(ids)).map_err(internal)?;
    let challenge = save_challenge(&state,merchant.0,"REGISTER",Some(session_hash(&headers)?),&registration).await?;
    Ok(Json(json!({"challenge_id":challenge,"options":options})))
}

#[derive(Deserialize)]
pub struct RegistrationFinish { challenge_id: Uuid, credential: RegisterPublicKeyCredential }

pub async fn register_finish(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<RegistrationFinish>) -> Result<Json<Value>,ApiError> {
    let merchant = authenticate_dashboard(&state,&headers).await?;
    // A matching challenge is consumed before verification, including failures.
    let stored: Option<Value> = sqlx::query_scalar("DELETE FROM merchant_passkey_challenges WHERE id=$1 AND merchant_id=$2 AND kind='REGISTER' AND session_hash=$3 AND expires_at>now() RETURNING state")
        .bind(input.challenge_id).bind(merchant.0).bind(session_hash(&headers)?).fetch_optional(state.store.pool()).await.map_err(db)?;
    let registration: PasskeyRegistration = serde_json::from_value(stored.ok_or_else(|| ceremony_error("expired challenge"))?).map_err(internal)?;
    let passkey = webauthn()?.finish_passkey_registration(&input.credential,&registration).map_err(ceremony_error)?;
    let id = URL_SAFE_NO_PAD.encode(passkey.cred_id().as_ref());
    let inserted = sqlx::query("INSERT INTO merchant_passkeys(credential_id,merchant_id,passkey) VALUES($1,$2,$3) ON CONFLICT(credential_id) DO NOTHING")
        .bind(id).bind(merchant.0).bind(serde_json::to_value(passkey).map_err(internal)?)
        .execute(state.store.pool()).await.map_err(db)?;
    if inserted.rows_affected()!=1 {return Err(ApiError::bad("passkey_already_registered","This passkey is already registered. Use a different authenticator."));}
    Ok(Json(json!({"registered":true})))
}

#[derive(Deserialize)]
pub struct LoginStart { email: String }

pub async fn login_start(State(state): State<AppState>, Json(input): Json<LoginStart>) -> Result<Json<Value>,ApiError> {
    let email = input.email.trim().to_ascii_lowercase();
    if email.len()>254||!email.contains('@') {return Err(ApiError::bad("invalid_email","Enter your email address."));}
    let rows = sqlx::query("SELECT p.merchant_id,p.passkey FROM merchant_passkeys p JOIN merchants m ON m.id=p.merchant_id WHERE lower(m.email)=$1 AND m.email_verified_at IS NOT NULL")
        .bind(email).fetch_all(state.store.pool()).await.map_err(db)?;
    let first=rows.first().ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED,"passkey_not_available","No passkey is available for this account. Sign in with your email instead."))?;
    let merchant: Uuid = first.get("merchant_id");
    let keys = rows.iter().map(|row|serde_json::from_value::<Passkey>(row.get("passkey"))).collect::<Result<Vec<_>,_>>().map_err(internal)?;
    let (options,authentication) = webauthn()?.start_passkey_authentication(&keys).map_err(internal)?;
    let challenge = save_challenge(&state,merchant,"LOGIN",None,&authentication).await?;
    Ok(Json(json!({"challenge_id":challenge,"options":options})))
}

#[derive(Deserialize)]
pub struct LoginFinish { challenge_id: Uuid, credential: PublicKeyCredential }

pub async fn login_finish(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<LoginFinish>) -> Result<Json<Value>,ApiError> {
    let row = sqlx::query("DELETE FROM merchant_passkey_challenges WHERE id=$1 AND kind='LOGIN' AND expires_at>now() RETURNING merchant_id,state")
        .bind(input.challenge_id).fetch_optional(state.store.pool()).await.map_err(db)?
        .ok_or_else(||ceremony_error("expired challenge"))?;
    let merchant: Uuid = row.get("merchant_id");
    let authentication: PasskeyAuthentication = serde_json::from_value(row.get("state")).map_err(internal)?;
    let result = webauthn()?.finish_passkey_authentication(&input.credential,&authentication).map_err(ceremony_error)?;
    let id = URL_SAFE_NO_PAD.encode(result.cred_id().as_ref());
    let mut transaction=state.store.pool().begin().await.map_err(db)?;
    let stored: Value = sqlx::query_scalar("SELECT passkey FROM merchant_passkeys WHERE credential_id=$1 AND merchant_id=$2 FOR UPDATE")
        .bind(&id).bind(merchant).fetch_one(&mut *transaction).await.map_err(db)?;
    let mut passkey: Passkey = serde_json::from_value(stored).map_err(internal)?;
    passkey.update_credential(&result);
    sqlx::query("UPDATE merchant_passkeys SET passkey=$2,last_used_at=now() WHERE credential_id=$1")
        .bind(id).bind(serde_json::to_value(passkey).map_err(internal)?)
        .execute(&mut *transaction).await.map_err(db)?;
    transaction.commit().await.map_err(db)?;
    let agent=headers.get("user-agent").and_then(|v|v.to_str().ok()).map(str::to_owned);
    let (token,expiry)=create_session(&state,merchant,agent).await?;
    Ok(Json(json!({"session_token":token,"expires_at":expiry.unix_timestamp()*1000,"merchant":load_merchant(&state,merchant).await?})))
}
