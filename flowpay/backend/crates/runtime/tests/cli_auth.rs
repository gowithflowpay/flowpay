use axum::{extract::State, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use ed25519_dalek::{Signer, SigningKey};
use flowpay_runtime::{cli_auth, config::Config, state::AppState};
use serde_json::{json, Value};

async fn challenge(state: &AppState, key: &SigningKey, purpose: &str, email: &str) -> Value {
    let request = serde_json::from_value(json!({
        "public_key": B64.encode(key.verifying_key().to_bytes()), "purpose":purpose,
        "profile":{"business_name":"First User Store","email":email,"settlement_address":"0x1111111111111111111111111111111111111111"}
    })).unwrap();
    cli_auth::challenge(State(state.clone()), Json(request))
        .await
        .unwrap()
        .0
}

async fn finish(
    state: &AppState,
    challenge: &Value,
    key: &SigningKey,
    message: Option<&str>,
) -> Result<Value, flowpay_runtime::error::ApiError> {
    let signature = key.sign(
        message
            .unwrap_or(challenge["message"].as_str().unwrap())
            .as_bytes(),
    );
    let request = serde_json::from_value(
        json!({"id":challenge["id"],"signature":B64.encode(signature.to_bytes())}),
    )
    .unwrap();
    cli_auth::finish(State(state.clone()), Json(request))
        .await
        .map(|v| v.0)
}

#[tokio::test]
#[ignore = "requires a dedicated migrated database and test configuration"]
async fn device_key_signup_login_replay_and_account_ownership() {
    let database = std::env::var("FLOWPAY_CLI_AUTH_TEST_DATABASE_URL")
        .expect("dedicated test database required");
    assert!(
        database.contains("flowpay_cli_auth_test"),
        "use a dedicated CLI authentication test database"
    );
    let mut config = Config::from_env().unwrap();
    config.database_url = database;
    let state = AppState::build(config).await.unwrap();
    let email = format!("cli-{}@example.test", uuid::Uuid::now_v7());
    let key = SigningKey::from_bytes(&[71; 32]);
    let public = B64.encode(key.verifying_key().to_bytes());
    // This database is exclusively for this test. Remove this test identity on reruns.
    sqlx::query("DELETE FROM merchants WHERE id IN (SELECT merchant_id FROM merchant_cli_keys WHERE public_key=$1)")
        .bind(&public).execute(state.store.pool()).await.unwrap();
    let registered = challenge(&state, &key, "REGISTER", &email).await;
    let account = finish(&state, &registered, &key, None).await.unwrap();
    assert_eq!(account["merchant"]["email_verified"], false);
    assert_eq!(account["merchant"]["onboarding_completed"], true);
    assert_eq!(account["merchant"]["email"], email);
    assert!(account["session_token"].as_str().unwrap().len() > 30);
    assert_eq!(
        finish(&state, &registered, &key, None)
            .await
            .unwrap_err()
            .code,
        "device_auth_failed"
    );
    let login = challenge(&state, &key, "LOGIN", &email).await;
    let signed_in = finish(&state, &login, &key, None).await.unwrap();
    assert_eq!(signed_in["merchant"]["id"], account["merchant"]["id"]);
    let tampered = challenge(&state, &key, "LOGIN", &email).await;
    assert_eq!(
        finish(&state, &tampered, &key, Some("altered challenge"))
            .await
            .unwrap_err()
            .code,
        "device_auth_failed"
    );
    assert!(finish(&state, &tampered, &key, None).await.is_err());
    let attacker = SigningKey::from_bytes(&[72; 32]);
    let wrong_key = challenge(&state, &key, "LOGIN", &email).await;
    assert!(finish(&state, &wrong_key, &attacker, None).await.is_err());
    let takeover = challenge(&state, &attacker, "REGISTER", &email).await;
    assert_eq!(
        finish(&state, &takeover, &attacker, None)
            .await
            .unwrap_err()
            .code,
        "account_exists"
    );
    let expired = challenge(&state, &attacker, "LOGIN", &email).await;
    sqlx::query("UPDATE cli_key_challenges SET expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(uuid::Uuid::parse_str(expired["id"].as_str().unwrap()).unwrap())
        .execute(state.store.pool())
        .await
        .unwrap();
    assert!(finish(&state, &expired, &attacker, None).await.is_err());
    sqlx::query("UPDATE merchant_cli_keys SET revoked_at=now() WHERE public_key=$1")
        .bind(&public)
        .execute(state.store.pool())
        .await
        .unwrap();
    let revoked = challenge(&state, &key, "LOGIN", &email).await;
    assert!(finish(&state, &revoked, &key, None).await.is_err());
    sqlx::query("DELETE FROM merchants WHERE id=$1")
        .bind(uuid::Uuid::parse_str(account["merchant"]["id"].as_str().unwrap()).unwrap())
        .execute(state.store.pool())
        .await
        .unwrap();
}
