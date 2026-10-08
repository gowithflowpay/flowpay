//! Merchant identity: password hashing, session tokens, email verification
//! codes, and transactional email delivery over SMTP.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, SaltString},
    Argon2, PasswordVerifier,
};
use lettre::{
    message::Mailbox,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use rand::Rng;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("could not hash the password")]
    Hash,
    #[error("email delivery failed: {0}")]
    Mail(String),
    #[error("email is not configured")]
    MailNotConfigured,
}

/// Argon2 is the default password hash here. Verification is infallible from the
/// caller's perspective: a malformed stored hash simply fails to match.
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::Hash)
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// 256 bits of entropy, hex encoded, used as the session bearer token.
pub fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    hex::encode(bytes)
}

/// Sessions and verification codes are stored hashed, so a database leak does
/// not hand over a usable credential.
pub fn hash_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

/// A six digit verification code. Uniformly sampled so no digit is favoured.
pub fn new_verification_code() -> String {
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1_000_000u32))
}

/// Compares a submitted code against a stored hash without leaking timing.
pub fn code_matches(submitted: &str, stored_hash: &str) -> bool {
    use subtle::ConstantTimeEq;
    let expected = hash_secret(submitted);
    expected.as_bytes().ct_eq(stored_hash.as_bytes()).unwrap_u8() == 1
}

#[derive(Clone, Debug)]
pub struct Mailer {
    host: String,
    port: u16,
    username: String,
    password: String,
    from_address: String,
    from_name: String,
}

impl Mailer {
    pub fn new(
        host: String,
        port: u16,
        username: String,
        password: String,
        from_address: String,
        from_name: String,
    ) -> Self {
        Self {
            host,
            port,
            username,
            password,
            from_address,
            from_name,
        }
    }

    /// Builds a mailer from configuration, or `None` when SMTP is not set up.
    #[must_use]
    pub fn from_config(config: &crate::config::Config) -> Option<Self> {
        Some(Self::new(
            config.smtp_host.clone()?,
            config.smtp_port,
            config.smtp_username.clone()?,
            config.smtp_password.clone()?,
            config.mail_from.clone(),
            config.mail_from_name.clone(),
        ))
    }

    pub async fn send(&self, to: &str, subject: &str, html: String) -> Result<(), AuthError> {
        let from = Mailbox::new(Some(self.from_name.clone()), self.from_address.parse().map_err(
            |error| AuthError::Mail(format!("invalid sender address: {error}")),
        )?);
        let to = Mailbox::new(None, to.parse().map_err(|error| {
            AuthError::Mail(format!("invalid recipient address: {error}"))
        })?);
        let email = Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .header(lettre::message::header::ContentType::TEXT_HTML)
            .body(html)
            .map_err(|error| AuthError::Mail(error.to_string()))?;
        let builder = if self.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host)
        }.map_err(|error| AuthError::Mail(error.to_string()))?;
        let transport = builder
            .port(self.port)
            .timeout(Some(std::time::Duration::from_secs(15)))
            .credentials(Credentials::new(
                self.username.clone(),
                self.password.clone(),
            ))
            .build();
        transport
            .send(email)
            .await
            .map(|_| ())
            .map_err(|error| AuthError::Mail(error.to_string()))
    }
}

/// Shared template so signup and login mail look consistent.
#[must_use]
pub fn code_email_html(heading: &str, intro: &str, code: &str, minutes: i64) -> String {
    format!(
        r#"<div style="font-family:Inter,Segoe UI,Helvetica,Arial,sans-serif;background:#f6f5fb;padding:32px">
  <div style="max-width:520px;margin:0 auto;background:#fff;border-radius:18px;padding:32px">
    <p style="margin:0 0 20px;font-size:13px;font-weight:700;color:#5a32e6;letter-spacing:.08em">FLOWPAY</p>
    <h1 style="margin:0 0 12px;font-size:22px;color:#16152a">{heading}</h1>
    <p style="margin:0 0 24px;font-size:14px;line-height:1.6;color:#6f6e80">{intro}</p>
    <div style="padding:18px;border-radius:12px;background:#f6f5fb;text-align:center">
      <span style="font-size:32px;font-weight:700;letter-spacing:.32em;color:#16152a">{code}</span>
    </div>
    <p style="margin:20px 0 0;font-size:12px;line-height:1.6;color:#8a8899">
      This code expires in {minutes} minutes. If you did not request it, you can ignore this email.
    </p>
  </div>
</div>"#
    )
}
