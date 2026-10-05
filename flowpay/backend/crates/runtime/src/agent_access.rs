//! Headless CLI/agent access: device-flow bootstrap, wallet linking, and a
//! payment event stream. Everything rides on the existing payment engine,
//! merchant identity, and API-key model — no parallel state machines.

use crate::auth::hash_secret;
use crate::error::ApiError;
use crate::routes::{authenticate, authenticate_dashboard, authenticate_scoped, db, map_store};
use crate::state::AppState;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use flowpay_claims::{verify_eip191_signature, WalletChallenge};
use flowpay_domain::{ClaimId, MerchantId, PaymentState};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sha3::Keccak256;
use sqlx::Row;
use std::convert::Infallible;
use std::time::Duration as StdDuration;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

fn db_err(e: sqlx::Error) -> ApiError {
    db(e)
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        e.to_string(),
    )
}

fn normalize_address(value: &str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    let raw = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    if raw.len() != 40 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::bad(
            "invalid_address",
            "wallet address must be a 20-byte hex EVM address",
        ));
    }
    Ok(format!("0x{}", raw.to_ascii_lowercase()))
}

/// EIP-55 mixed-case checksum encoding (keccak of the lowercase hex address).
fn checksum_address(addr: &str) -> String {
    let raw = addr.trim_start_matches("0x").to_ascii_lowercase();
    let hash = Keccak256::digest(raw.as_bytes());
    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, ch) in raw.chars().enumerate() {
        let nibble = if i % 2 == 0 {
            hash[i / 2] >> 4
        } else {
            hash[i / 2] & 0x0f
        };
        if nibble >= 8 && ch.is_ascii_alphabetic() {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn generate_user_code() -> String {
    // Alphabet without ambiguous characters (no I, L, O, U, 0, 1).
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 8];
    rng.fill_bytes(&mut bytes);
    let mut out = String::with_capacity(9);
    for (i, b) in bytes.iter().enumerate() {
        if i == 4 {
            out.push('-');
        }
        out.push(ALPHABET[(*b as usize) % ALPHABET.len()] as char);
    }
    out
}

fn generate_device_code() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Seals the one-time API credential with AES-256-GCM. The key is derived from
/// the existing API-key pepper, so no new secret material is introduced.
fn seal_credential(config: &crate::config::Config, plaintext: &str) -> Result<Vec<u8>, ApiError> {
    let mut hasher = Sha256::new();
    hasher.update(b"flowpay:device-grant:v1:");
    hasher.update(config.api_key_pepper.as_bytes());
    let key_bytes = hasher.finalize();
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map(|ciphertext| {
            let mut sealed = nonce.to_vec();
            sealed.extend(ciphertext);
            sealed
        })
        .map_err(|_| internal("failed to seal device grant credential"))
}

fn open_credential(config: &crate::config::Config, sealed: &[u8]) -> Result<String, ApiError> {
    if sealed.len() < 12 {
        return Err(internal("corrupt device grant credential"));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"flowpay:device-grant:v1:");
    hasher.update(config.api_key_pepper.as_bytes());
    let key_bytes = hasher.finalize();
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let (nonce, ciphertext) = sealed.split_at(12);
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| internal("failed to open device grant credential"))
        .map(|plaintext| String::from_utf8_lossy(&plaintext).into_owned())
}

// ---------------------------------------------------------------------------
// Device flow: POST /v1/cli/device/start | /poll | /approve | /deny
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DeviceStartRequest {
    pub name: Option<String>,
    pub scopes: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct DeviceStartResponse {
    pub device_code: String,
    pub user_code: String,
    pub expires_in: i64,
    pub interval: u64,
    pub verification_uri: String,
}

fn device_grant_ttl(config: &crate::config::Config) -> Duration {
    Duration::seconds(config.device_grant_ttl_seconds.max(60))
}

pub async fn device_start(
    State(state): State<AppState>,
    Json(req): Json<DeviceStartRequest>,
) -> Result<(StatusCode, Json<DeviceStartResponse>), ApiError> {
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("flowpay cli")
        .chars()
        .take(80)
        .collect::<String>();
    let allowed = [
        "payments:read",
        "payments:write",
        "claims:read",
        "claims:write",
        "webhooks:read",
    ];
    let scopes: Vec<String> = match req.scopes {
        Some(list) if !list.is_empty() => {
            if list.iter().any(|s| !allowed.contains(&s.as_str())) {
                return Err(ApiError::bad(
                    "invalid_scopes",
                    "one or more requested scopes are unsupported",
                ));
            }
            list
        }
        _ => allowed.iter().map(|s| s.to_string()).collect(),
    };
    let device_code = generate_device_code();
    let user_code = generate_user_code();
    let ttl = device_grant_ttl(&state.config);
    let expires_at = now() + ttl;
    sqlx::query(
        "INSERT INTO device_grants(id,device_code_hash,user_code,name,scopes,state,expires_at) VALUES($1,$2,$3,$4,$5,'PENDING',$6)",
    )
    .bind(Uuid::now_v7())
    .bind(hash_secret(&device_code))
    .bind(&user_code)
    .bind(&name)
    .bind(&scopes)
    .bind(expires_at)
    .execute(state.store.pool())
    .await
    .map_err(db_err)?;
    Ok((
        StatusCode::CREATED,
        Json(DeviceStartResponse {
            device_code,
            user_code,
            expires_in: ttl.whole_seconds(),
            interval: 2,
            verification_uri: format!(
                "{}/dashboard/devices",
                state.config.dashboard_base_url.trim_end_matches('/')
            ),
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct DevicePollRequest {
    pub device_code: String,
}

const DEVICE_GRANT_COLUMNS: &str =
    "g.id, g.state, g.poll_count, g.api_key_cipher, m.email AS merchant_email";

async fn fetch_device_grant(
    state: &AppState,
    device_code_hash: &str,
) -> Result<Option<sqlx::postgres::PgRow>, ApiError> {
    let sql = format!(
        "SELECT {DEVICE_GRANT_COLUMNS} FROM device_grants g LEFT JOIN merchants m ON m.id=g.merchant_id WHERE g.device_code_hash=$1"
    );
    sqlx::query(&sql)
        .bind(device_code_hash)
        .fetch_optional(state.store.pool())
        .await
        .map_err(db_err)
}

pub async fn device_poll(
    State(state): State<AppState>,
    Json(req): Json<DevicePollRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let device_code_hash = hash_secret(&req.device_code);
    let Some(row) = fetch_device_grant(&state, &device_code_hash).await? else {
        return Err(ApiError::bad(
            "expired_token",
            "device grant is unknown, expired, or already consumed",
        ));
    };
    let grant_state: String = row.try_get("state").map_err(internal)?;
    match grant_state.as_str() {
        "PENDING" => {
            let poll_count: i32 = row.try_get("poll_count").map_err(internal)?;
            if i64::from(poll_count) + 1 > i64::from(state.config.device_grant_max_polls) {
                sqlx::query(
                    "UPDATE device_grants SET state='EXPIRED' WHERE id=$1 AND state='PENDING'",
                )
                .bind(row.try_get::<Uuid, _>("id").map_err(internal)?)
                .execute(state.store.pool())
                .await
                .map_err(db_err)?;
                return Err(ApiError::bad(
                    "slow_down",
                    "too many polls; start a new device grant",
                ));
            }
            sqlx::query("UPDATE device_grants SET poll_count=poll_count+1 WHERE id=$1")
                .bind(row.try_get::<Uuid, _>("id").map_err(internal)?)
                .execute(state.store.pool())
                .await
                .map_err(db_err)?;
            Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "authorization_pending",
                "the human has not approved this device yet",
            ))
        }
        "APPROVED" => {
            let sealed: Option<Vec<u8>> = row.try_get("api_key_cipher").ok().flatten();
            let Some(sealed) = sealed else {
                return Err(ApiError::bad(
                    "authorization_pending",
                    "approval is still being finalized",
                ));
            };
            let api_key = open_credential(&state.config, &sealed)?;
            // One-time delivery: the grant is consumed immediately after the
            // credential is read, so a repeated poll cannot replay it.
            let consumed = sqlx::query("UPDATE device_grants SET state='CONSUMED', api_key_cipher=NULL WHERE id=$1 AND state='APPROVED' AND expires_at>now()")
                .bind(row.try_get::<Uuid, _>("id").map_err(internal)?)
                .execute(state.store.pool())
                .await
                .map_err(db_err)?;
            if consumed.rows_affected() != 1 {
                return Err(ApiError::bad(
                    "expired_token",
                    "device grant was already consumed or expired",
                ));
            }
            let email: Option<String> = row.try_get("merchant_email").ok().flatten();
            Ok(Json(serde_json::json!({
                "status": "APPROVED",
                "api_key": api_key,
                "merchant_email": email,
            })))
        }
        "DENIED" => Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "access_denied",
            "the device grant was denied",
        )),
        _ => Err(ApiError::bad(
            "expired_token",
            "device grant is unknown, expired, or already consumed",
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct DeviceUserCodeRequest {
    pub user_code: String,
}

async fn find_pending_grant(
    state: &AppState,
    user_code: &str,
) -> Result<(Uuid, String, Option<Uuid>), ApiError> {
    let row = sqlx::query(
        "SELECT id,name,merchant_id FROM device_grants WHERE user_code=$1 AND state='PENDING' AND expires_at>now()",
    )
    .bind(user_code.trim().to_ascii_uppercase())
    .fetch_optional(state.store.pool())
    .await
    .map_err(db_err)?;
    let Some(row) = row else {
        return Err(ApiError::bad(
            "invalid_user_code",
            "no pending device grant matches this code",
        ));
    };
    let id: Uuid = row.try_get("id").map_err(internal)?;
    let name: String = row.try_get("name").map_err(internal)?;
    let merchant_id: Option<Uuid> = row.try_get("merchant_id").ok().flatten();
    Ok((id, name, merchant_id))
}

pub async fn device_approve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeviceUserCodeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let merchant = authenticate_dashboard(&state, &headers).await?;
    let (grant_id, name, existing) = find_pending_grant(&state, &req.user_code).await?;
    if let Some(existing) = existing {
        if existing != merchant.0 {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "grant_mismatch",
                "this device grant was started for a different account",
            ));
        }
    }
    // Reuse the existing API key model so CLI credentials are ordinary keys:
    // peppered hash at rest, listed and revocable from the dashboard.
    let key_id = Uuid::now_v7();
    let prefix = format!("fp_cli_{}", &Uuid::now_v7().simple().to_string()[..10]);
    let secret = format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple());
    let full = format!("{prefix}.{secret}");
    let hash = hex::encode(Sha256::digest(
        [state.config.api_key_pepper.as_bytes(), full.as_bytes()].concat(),
    ));
    let scopes: Vec<String> = sqlx::query_scalar("SELECT scopes FROM device_grants WHERE id=$1")
        .bind(grant_id)
        .fetch_one(state.store.pool())
        .await
        .map_err(db_err)?;
    let mut tx = state.store.pool().begin().await.map_err(db_err)?;
    sqlx::query("INSERT INTO api_keys(id,merchant_id,label,public_prefix,secret_hash,scopes) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(key_id)
        .bind(merchant.0)
        .bind(format!("CLI: {name}"))
        .bind(&prefix)
        .bind(&hash)
        .bind(&scopes)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
    let sealed = seal_credential(&state.config, &full)?;
    let approved = sqlx::query(
        "UPDATE device_grants SET state='APPROVED',merchant_id=$2,api_key_id=$3,api_key_cipher=$4,approved_at=now() WHERE id=$1 AND state='PENDING' AND expires_at>now()",
    )
    .bind(grant_id)
    .bind(merchant.0)
    .bind(key_id)
    .bind(&sealed)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    if approved.rows_affected() != 1 {
        return Err(ApiError::bad(
            "invalid_user_code",
            "device grant was already decided or expired",
        ));
    }
    tx.commit().await.map_err(db_err)?;
    Ok(Json(serde_json::json!({
        "approved": true,
        "device_name": name,
        "prefix": prefix,
    })))
}

pub async fn device_deny(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeviceUserCodeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _merchant = authenticate_dashboard(&state, &headers).await?;
    let (grant_id, _, _) = find_pending_grant(&state, &req.user_code).await?;
    sqlx::query(
        "UPDATE device_grants SET state='DENIED',denied_at=now() WHERE id=$1 AND state='PENDING'",
    )
    .bind(grant_id)
    .execute(state.store.pool())
    .await
    .map_err(db_err)?;
    Ok(Json(serde_json::json!({"denied": true})))
}

// ---------------------------------------------------------------------------
// Wallet linking: POST /v1/wallets/challenge | /verify | GET list | DELETE
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct WalletChallengeResponse {
    pub nonce: String,
    pub message: String,
    pub expires_in: i64,
}

#[derive(Debug, Deserialize)]
pub struct WalletChallengeRequest {
    pub address: String,
}

async fn merchant_email(state: &AppState, merchant: MerchantId) -> Result<String, ApiError> {
    let email: Option<String> = sqlx::query_scalar("SELECT email FROM merchants WHERE id=$1")
        .bind(merchant.0)
        .fetch_optional(state.store.pool())
        .await
        .map_err(db_err)?
        .flatten();
    Ok(email.unwrap_or_else(|| "unknown account".into()))
}

pub async fn wallet_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WalletChallengeRequest>,
) -> Result<(StatusCode, Json<WalletChallengeResponse>), ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let address = normalize_address(&req.address)?;
    let ttl = Duration::seconds(state.config.wallet_challenge_ttl_seconds.max(30));
    let expires_at = now() + ttl;
    let nonce = Uuid::now_v7().simple().to_string();
    let message = format!(
        "FlowPay wallet linking\nAccount: {account}\nWallet: {wallet}\nNonce: {nonce}\nExpires: {expires_ts}\n\nSigning proves wallet control. It does not authorize any transfer.",
        account = merchant_email(&state, merchant).await?,
        wallet = checksum_address(&address),
        nonce = nonce,
        expires_ts = expires_at.unix_timestamp(),
    );
    sqlx::query(
        "INSERT INTO wallet_challenges(id,wallet_address,nonce_hash,message,merchant_id,expires_at) VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(Uuid::now_v7())
    .bind(&address)
    .bind(hash_secret(&nonce))
    .bind(&message)
    .bind(merchant.0)
    .bind(expires_at)
    .execute(state.store.pool())
    .await
    .map_err(db_err)?;
    Ok((
        StatusCode::CREATED,
        Json(WalletChallengeResponse {
            nonce,
            message,
            expires_in: ttl.whole_seconds(),
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct WalletVerifyRequest {
    pub address: String,
    pub nonce: String,
    pub signature: String,
    pub label: Option<String>,
}

pub async fn wallet_verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WalletVerifyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let address = normalize_address(&req.address)?;
    // Consume the challenge atomically: exactly one verify call wins.
    let row = sqlx::query(
        "UPDATE wallet_challenges SET consumed_at=now() WHERE nonce_hash=$1 AND merchant_id=$2 AND consumed_at IS NULL AND expires_at>now() RETURNING message",
    )
    .bind(hash_secret(&req.nonce))
    .bind(merchant.0)
    .fetch_optional(state.store.pool())
    .await
    .map_err(db_err)?;
    let Some(row) = row else {
        return Err(ApiError::bad(
            "invalid_challenge",
            "challenge is unknown, expired, or already used",
        ));
    };
    let message: String = row.try_get("message").map_err(internal)?;
    // The claims-crate verifier checks expiry itself, so allow a generous
    // window here — the database expiry above is authoritative.
    let challenge = WalletChallenge {
        claim_id: ClaimId::new(),
        nonce: req.nonce.clone(),
        message: message.clone(),
        expires_at: now() + Duration::minutes(5),
    };
    let verification = match verify_eip191_signature(&challenge, &address, &req.signature, now()) {
        Ok(v) => v,
        Err(flowpay_claims::SignatureError::Expired) | Err(_) => {
            return Err(ApiError::bad(
                "invalid_signature",
                "signature did not verify for this wallet",
            ))
        }
    };
    if !verification.verified {
        return Err(ApiError::bad(
            "invalid_signature",
            "signature did not verify for this wallet",
        ));
    }
    let label = req
        .label
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.chars().take(80).collect::<String>());
    sqlx::query(
        "INSERT INTO linked_wallets(id,merchant_id,wallet_address,label) VALUES($1,$2,$3,$4) ON CONFLICT (merchant_id,wallet_address) DO UPDATE SET unlinked_at=NULL,label=COALESCE(EXCLUDED.label,linked_wallets.label)",
    )
    .bind(Uuid::now_v7())
    .bind(merchant.0)
    .bind(&address)
    .bind(&label)
    .execute(state.store.pool())
    .await
    .map_err(db_err)?;
    Ok(Json(serde_json::json!({
        "verified": true,
        "address": checksum_address(&address),
        "label": label,
    })))
}

pub async fn wallet_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let rows = sqlx::query(
        "SELECT wallet_address,label,linked_at FROM linked_wallets WHERE merchant_id=$1 AND unlinked_at IS NULL ORDER BY linked_at DESC",
    )
    .bind(merchant.0)
    .fetch_all(state.store.pool())
    .await
    .map_err(db_err)?;
    Ok(Json(serde_json::json!({
        "data": rows.iter().map(|r| serde_json::json!({
            "address": checksum_address(&r.try_get::<String, _>("wallet_address").unwrap_or_default()),
            "label": r.try_get::<Option<String>, _>("label").ok().flatten(),
            "linked_at": r.try_get::<OffsetDateTime, _>("linked_at").ok().map(|t| t.unix_timestamp() * 1000),
        })).collect::<Vec<_>>()
    })))
}

#[derive(Debug, Deserialize)]
pub struct WalletUnlinkRequest {
    pub address: String,
}

pub async fn wallet_unlink(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WalletUnlinkRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let merchant = authenticate(&state, &headers).await?;
    let address = normalize_address(&req.address)?;
    let result = sqlx::query(
        "UPDATE linked_wallets SET unlinked_at=now() WHERE merchant_id=$1 AND wallet_address=$2 AND unlinked_at IS NULL",
    )
    .bind(merchant.0)
    .bind(&address)
    .execute(state.store.pool())
    .await
    .map_err(db_err)?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(
        serde_json::json!({"unlinked": true, "address": checksum_address(&address)}),
    ))
}

// ---------------------------------------------------------------------------
// Payment wait/poll helper shared by the SSE endpoint
// ---------------------------------------------------------------------------

/// Terminal from a payer's perspective. RECOVERY_AVAILABLE and RECOVERY_PENDING
/// are intentionally NOT terminal-success: they end the wait with a recoverable
/// classification instead of being silently treated as COMPLETED.
fn stream_terminal(state: PaymentState) -> bool {
    matches!(
        state,
        PaymentState::Completed
            | PaymentState::Recovered
            | PaymentState::Cancelled
            | PaymentState::Escalated
            | PaymentState::Expired
            | PaymentState::Failed
            | PaymentState::RecoveryAvailable
            | PaymentState::RecoveryPending
    )
}

// ---------------------------------------------------------------------------
// Payment event stream: GET /v1/payments/{id}/events (SSE)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PaymentEventsQuery {
    pub timeout_seconds: Option<i64>,
}

pub async fn payment_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    query: axum::extract::Query<PaymentEventsQuery>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let merchant = authenticate_scoped(&state, &headers, Some("payments:read")).await?;
    let timeout_seconds = query.timeout_seconds.unwrap_or(1800).clamp(5, 86_400);
    // Authorize before opening the stream; 404/403 must be HTTP, not SSE.
    state
        .store
        .get_payment(merchant, &id)
        .await
        .map_err(map_store)?;
    let store = state.store.clone();
    let deadline = tokio::time::Instant::now() + StdDuration::from_secs(timeout_seconds as u64);
    let stream = futures_util::stream::unfold(
        (store, merchant, id, None::<String>, false, deadline),
        |(store, merchant, id, last_status, done, deadline)| async move {
            if done {
                return None;
            }
            loop {
                if tokio::time::Instant::now() >= deadline {
                    return Some((
                        Ok(Event::default().event("timeout").data("{\"timeout\":true}")),
                        (store, merchant, id, last_status, true, deadline),
                    ));
                }
                match store.get_payment(merchant, &id).await {
                    Err(flowpay_persistence::StoreError::NotFound) => {
                        return Some((
                            Ok(Event::default()
                                .event("error")
                                .data("{\"error\":\"not_found\"}")),
                            (store, merchant, id, last_status, true, deadline),
                        ));
                    }
                    Err(_) => {
                        // Transient: keep the stream alive and retry.
                        tokio::time::sleep(StdDuration::from_secs(2)).await;
                        continue;
                    }
                    Ok(payment) => {
                        let status = payment.state.as_str().to_owned();
                        if Some(&status) != last_status.as_ref() {
                            let payload = serde_json::json!({
                                "id": payment.public_id,
                                "status": status,
                                "amount_atomic": payment.expected_amount.to_string(),
                                "asset": payment.expected_asset.symbol,
                                "chain": payment.expected_chain.to_string(),
                                "address": payment.checkout_address.value,
                            });
                            let terminal = stream_terminal(payment.state);
                            return Some((
                                Ok(Event::default()
                                    .event("payment.status")
                                    .data(payload.to_string())),
                                (store, merchant, id, Some(status), terminal, deadline),
                            ));
                        }
                        // Unchanged status: keep polling silently.
                        tokio::time::sleep(StdDuration::from_secs(2)).await;
                    }
                }
            }
        },
    );
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
