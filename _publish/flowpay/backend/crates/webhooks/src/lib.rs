use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WebhookEnvelope {
    pub id: String,
    pub event_type: String,
    pub created_at: i64,
    pub api_version: String,
    pub data: Value,
}

impl WebhookEnvelope {
    #[must_use]
    pub fn new(event_type: impl Into<String>, data: Value, now: OffsetDateTime) -> Self {
        Self {
            id: format!("evt_{}", Uuid::now_v7().simple()),
            event_type: event_type.into(),
            created_at: now.unix_timestamp(),
            api_version: "2026-08-01".into(),
            data,
        }
    }
}

#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("invalid signature header")]
    InvalidHeader,
    #[error("timestamp outside replay window")]
    TimestampOutsideWindow,
    #[error("signature mismatch")]
    SignatureMismatch,
}

#[must_use]
pub fn sign(secret: &[u8], timestamp: i64, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts arbitrary key lengths");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!(
        "t={timestamp},v1={}",
        hex::encode(mac.finalize().into_bytes())
    )
}

pub fn verify(
    secret: &[u8],
    signature_header: &str,
    body: &[u8],
    now: OffsetDateTime,
    tolerance: Duration,
) -> Result<(), WebhookError> {
    let mut timestamp = None;
    let mut signature = None;
    for part in signature_header.split(',') {
        if let Some(value) = part.strip_prefix("t=") {
            timestamp = value.parse::<i64>().ok();
        }
        if let Some(value) = part.strip_prefix("v1=") {
            signature = hex::decode(value).ok();
        }
    }
    let timestamp = timestamp.ok_or(WebhookError::InvalidHeader)?;
    let signature = signature.ok_or(WebhookError::InvalidHeader)?;
    let age = (now.unix_timestamp() - timestamp).unsigned_abs();
    if age > tolerance.whole_seconds().unsigned_abs() {
        return Err(WebhookError::TimestampOutsideWindow);
    }
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| WebhookError::InvalidHeader)?;
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&signature)
        .map_err(|_| WebhookError::SignatureMismatch)
}

#[must_use]
pub fn retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(8);
    Duration::seconds(5_i64.saturating_mul(1_i64 << exponent))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signature_round_trip() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::hours(1);
        let body = br#"{"id":"evt_1"}"#;
        let header = sign(b"secret", now.unix_timestamp(), body);
        assert!(verify(b"secret", &header, body, now, Duration::minutes(5)).is_ok());
        assert!(verify(b"wrong", &header, body, now, Duration::minutes(5)).is_err());
    }
    #[test]
    fn replay_is_rejected() {
        let signed_at = OffsetDateTime::UNIX_EPOCH;
        let header = sign(b"secret", signed_at.unix_timestamp(), b"{}");
        assert!(matches!(
            verify(
                b"secret",
                &header,
                b"{}",
                signed_at + Duration::minutes(6),
                Duration::minutes(5)
            ),
            Err(WebhookError::TimestampOutsideWindow)
        ));
    }
}

#[derive(Debug, Error)]
pub enum SecretBoxError {
    #[error("webhook encryption key must be exactly 32 bytes")]
    InvalidKey,
    #[error("ciphertext is invalid")]
    InvalidCiphertext,
    #[error("webhook secret encryption/decryption failed")]
    Crypto,
}

#[must_use]
pub fn generate_signing_secret() -> String {
    use rand::RngCore;
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    format!("whsec_{}", hex::encode(bytes))
}

pub fn encrypt_secret(master_key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SecretBoxError> {
    use aes_gcm::{
        aead::{rand_core::RngCore, Aead, OsRng},
        Aes256Gcm, KeyInit, Nonce,
    };
    if master_key.len() != 32 {
        return Err(SecretBoxError::InvalidKey);
    }
    let cipher = Aes256Gcm::new_from_slice(master_key).map_err(|_| SecretBoxError::InvalidKey)?;
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|_| SecretBoxError::Crypto)?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt_secret(master_key: &[u8], sealed: &[u8]) -> Result<Vec<u8>, SecretBoxError> {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    if master_key.len() != 32 {
        return Err(SecretBoxError::InvalidKey);
    }
    if sealed.len() < 13 {
        return Err(SecretBoxError::InvalidCiphertext);
    }
    let cipher = Aes256Gcm::new_from_slice(master_key).map_err(|_| SecretBoxError::InvalidKey)?;
    cipher
        .decrypt(Nonce::from_slice(&sealed[..12]), &sealed[12..])
        .map_err(|_| SecretBoxError::Crypto)
}

#[cfg(test)]
mod secret_tests {
    use super::*;
    #[test]
    fn secret_box_round_trip() {
        let key = [9_u8; 32];
        let sealed = encrypt_secret(&key, b"whsec_test").unwrap();
        assert_ne!(sealed, b"whsec_test");
        assert_eq!(decrypt_secret(&key, &sealed).unwrap(), b"whsec_test");
    }
}
