use flowpay_domain::ClaimId;
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WalletChallenge {
    pub claim_id: ClaimId,
    pub nonce: String,
    pub message: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WalletSignatureVerification {
    pub verified: bool,
    pub recovered_address: Option<String>,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error("challenge expired")]
    Expired,
    #[error("invalid signature encoding")]
    InvalidEncoding,
    #[error("signature recovery failed")]
    RecoveryFailed,
}

#[must_use]
pub fn new_wallet_challenge(
    claim_id: ClaimId,
    payment_public_id: &str,
    destination: &str,
    now: OffsetDateTime,
) -> WalletChallenge {
    let nonce = Uuid::now_v7().simple().to_string();
    let expires_at = now + Duration::minutes(15);
    let message = format!(
        "FlowPay recovery authorization\nClaim: {}\nPayment: {payment_public_id}\nDestination: {destination}\nNonce: {nonce}\nExpires: {}\n\nSigning proves wallet control. It does not authorize any transfer beyond this claim.",
        claim_id.0,
        expires_at.unix_timestamp()
    );
    WalletChallenge {
        claim_id,
        nonce,
        message,
        expires_at,
    }
}

pub fn verify_eip191_signature(
    challenge: &WalletChallenge,
    claimed_wallet: &str,
    signature_hex: &str,
    now: OffsetDateTime,
) -> Result<WalletSignatureVerification, SignatureError> {
    if now > challenge.expires_at {
        return Err(SignatureError::Expired);
    }
    let raw = hex::decode(signature_hex.trim_start_matches("0x"))
        .map_err(|_| SignatureError::InvalidEncoding)?;
    if raw.len() != 65 {
        return Err(SignatureError::InvalidEncoding);
    }
    let sig = Signature::try_from(&raw[..64]).map_err(|_| SignatureError::InvalidEncoding)?;
    let v = match raw[64] {
        27 | 28 => raw[64] - 27,
        0 | 1 => raw[64],
        _ => return Err(SignatureError::InvalidEncoding),
    };
    let recovery_id = RecoveryId::try_from(v).map_err(|_| SignatureError::InvalidEncoding)?;
    let prefix = format!("\x19Ethereum Signed Message:\n{}", challenge.message.len());
    let mut hasher = Keccak256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(challenge.message.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let key = VerifyingKey::recover_from_prehash(&digest, &sig, recovery_id)
        .map_err(|_| SignatureError::RecoveryFailed)?;
    let encoded = key.to_encoded_point(false);
    let public = encoded.as_bytes();
    let hash = Keccak256::digest(&public[1..]);
    let recovered = format!("0x{}", hex::encode(&hash[12..]));
    Ok(WalletSignatureVerification {
        verified: recovered.eq_ignore_ascii_case(claimed_wallet),
        recovered_address: Some(recovered.clone()),
        reason: if recovered.eq_ignore_ascii_case(claimed_wallet) {
            "wallet_signature_verified"
        } else {
            "signature_wallet_mismatch"
        }
        .to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowpay_domain::ClaimId;
    use k256::ecdsa::SigningKey;

    #[test]
    fn challenge_binds_destination() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let a = new_wallet_challenge(
            ClaimId::new(),
            "pay_1",
            "0x1111111111111111111111111111111111111111",
            now,
        );
        assert!(a
            .message
            .contains("0x1111111111111111111111111111111111111111"));
    }

    #[test]
    fn verifies_recoverable_wallet_signature() {
        let signing_key = SigningKey::from_bytes((&[7_u8; 32]).into()).unwrap();
        let verifying_key = signing_key.verifying_key();
        let public = verifying_key.to_encoded_point(false);
        let hash = Keccak256::digest(&public.as_bytes()[1..]);
        let wallet = format!("0x{}", hex::encode(&hash[12..]));
        let challenge = new_wallet_challenge(
            ClaimId::new(),
            "pay_test",
            &wallet,
            OffsetDateTime::UNIX_EPOCH,
        );
        let prefix = format!("\x19Ethereum Signed Message:\n{}", challenge.message.len());
        let mut hasher = Keccak256::new();
        hasher.update(prefix.as_bytes());
        hasher.update(challenge.message.as_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        let (sig, recid) = signing_key.sign_prehash_recoverable(&digest).unwrap();
        let mut bytes = Vec::from(sig.to_bytes().as_slice());
        bytes.push(recid.to_byte() + 27);
        let result = verify_eip191_signature(
            &challenge,
            &wallet,
            &format!("0x{}", hex::encode(bytes)),
            OffsetDateTime::UNIX_EPOCH,
        )
        .unwrap();
        assert!(result.verified);
    }
}
