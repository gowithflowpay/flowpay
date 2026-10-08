use flowpay_domain::ChainKey;
use flowpay_runtime::{config::{Config, ChainConfig}, routes, state::AppState};
use serde_json::Value;

#[tokio::test]
#[ignore = "requires a dedicated migrated database and test configuration"]
async fn catalog_includes_only_enabled_payment_assets_on_configured_chains() {
    let database = std::env::var("FLOWPAY_CLI_AUTH_TEST_DATABASE_URL").unwrap();
    assert!(database.contains("flowpay_cli_auth_test"));
    let mut config = Config::from_env().unwrap();
    config.database_url = database;
    config.chains.insert(ChainKey::Base, ChainConfig {
        chain: ChainKey::Base,
        rpc_url: "http://127.0.0.1:1".into(),
        numeric_chain_id: 31337,
        factory_address: "0x1111111111111111111111111111111111111111".into(),
    });
    let state = AppState::build(config).await.unwrap();
    let prefix = format!("CAT{}", uuid::Uuid::now_v7().simple());
    let mut ids = Vec::new();
    for (suffix, chain, purpose, enabled) in [
        ("PAY", "base", "PAYMENT", true),
        ("BOTH", "base", "BOTH", true),
        ("DISABLED", "base", "PAYMENT", false),
        ("RECOVERY", "base", "RECOVERY", true),
        ("UNCONFIGURED", "custom:catalog_test", "PAYMENT", true),
    ] {
        let id = uuid::Uuid::now_v7();
        let token = format!("0x{:040x}", id.as_u128());
        sqlx::query("INSERT INTO chain_assets(id,chain,symbol,token_contract,decimals,purpose,enabled) VALUES($1,$2,$3,$4,6,$5,$6)")
            .bind(id).bind(chain).bind(format!("{prefix}{suffix}")).bind(token).bind(purpose).bind(enabled)
            .execute(state.store.pool()).await.unwrap();
        ids.push(id);
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = routes::router(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let response = reqwest::get(format!("http://{address}/v1/payment-assets")).await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let symbols: Vec<_> = body["data"].as_array().unwrap().iter()
        .filter_map(|asset| asset["symbol"].as_str()).filter(|s| s.starts_with(&prefix)).collect();
    server.abort();
    sqlx::query("DELETE FROM chain_assets WHERE id=ANY($1)").bind(&ids)
        .execute(state.store.pool()).await.unwrap();
    assert_eq!(symbols.len(), 2);
    assert!(symbols.contains(&format!("{prefix}PAY").as_str()));
    assert!(symbols.contains(&format!("{prefix}BOTH").as_str()));
}
