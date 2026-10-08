use crate::config::Config;
use anyhow::Context;
use flowpay_chains::ChainIdentity;
use flowpay_domain::ChainKey;
use flowpay_evm::{Create3AddressDeriver, RpcEvmAdapter};
use flowpay_persistence::PgStore;
use sqlx::postgres::PgPoolOptions;
use std::{collections::HashMap, sync::Arc, time::Duration};

#[derive(Clone)]
pub struct ChainRuntime {
    pub adapter: Arc<RpcEvmAdapter>,
    pub deriver: Create3AddressDeriver,
    pub factory: String,
    pub rpc_url: String,
}

#[derive(Clone)]
pub struct AppState {
    pub store: PgStore,
    pub config: Config,
    pub chains: HashMap<ChainKey, ChainRuntime>,
    pub http: reqwest::Client,
}

impl AppState {
    pub async fn build(config: Config) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&config.database_url)
            .await
            .context("connect PostgreSQL")?;
        let store = PgStore::new(pool);
        let mut chains = HashMap::new();
        for (key, cc) in &config.chains {
            let deriver =
                Create3AddressDeriver::new(&cc.factory_address, &config.proxy_creation_code_hash)?;
            let adapter = RpcEvmAdapter::new(
                ChainIdentity {
                    key: key.clone(),
                    numeric_chain_id: Some(cc.numeric_chain_id),
                    genesis_hash: None,
                },
                cc.rpc_url.clone(),
                deriver.clone(),
                config.factory_runtime_code_hash.clone(),
                Some(config.operator_address.clone()),
            );
            chains.insert(
                key.clone(),
                ChainRuntime {
                    adapter: Arc::new(adapter),
                    deriver,
                    factory: cc.factory_address.clone(),
                    rpc_url: cc.rpc_url.clone(),
                },
            );
        }
        Ok(Self {
            store,
            config,
            chains,
            http: reqwest::Client::new(),
        })
    }
}
