use anyhow::{anyhow, Context};
use flowpay_domain::ChainKey;
use std::{collections::HashMap, env, str::FromStr};

#[derive(Clone, Debug)]
pub struct ChainConfig {
    pub chain: ChainKey,
    pub rpc_url: String,
    pub numeric_chain_id: u64,
    pub factory_address: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub database_url: String,
    pub environment: String,
    pub checkout_base_url: String,
    pub api_key_pepper: String,
    pub proxy_creation_code_hash: String,
    pub factory_runtime_code_hash: Option<String>,
    pub operator_address: String,
    pub operator_private_key: Option<String>,
    pub faucet_address: Option<String>,
    pub evidence_dir: String,
    pub webhook_encryption_key: Vec<u8>,
    pub chains: HashMap<ChainKey, ChainConfig>,
    pub agent_mode: String,
    pub model_provider: String,
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    pub openai_endpoint: String,
    pub agent_max_steps: usize,
    pub agent_retry_budget: usize,
    pub rabbitmq_url: String,
    pub provider_webhook_secret: Option<String>,
    pub provider_webhook_path: String,
    pub alchemy_api_key: Option<String>,
    pub alchemy_networks: Vec<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let factory = required("FLOWPAY_FACTORY_ADDRESS")?;
        let mut chains = HashMap::new();
        let is_local = env::var("FLOWPAY_ENV")
            .unwrap_or_else(|_| "local".into())
            .eq_ignore_ascii_case("local");
        if is_local {
            add_chain(&mut chains, ChainKey::Base, "BASE", 31337, &factory)?;
            add_chain(&mut chains, ChainKey::Bsc, "BSC", 31338, &factory)?;
        }
        if !is_local {
            add_chain(
                &mut chains,
                ChainKey::Custom("bsc_testnet".into()),
                "BSC_TESTNET",
                97,
                &factory,
            )?;
            add_chain(
                &mut chains,
                ChainKey::Custom("ethereum_sepolia".into()),
                "ETHEREUM_SEPOLIA",
                11155111,
                &factory,
            )?;
            add_chain(
                &mut chains,
                ChainKey::Custom("base_sepolia".into()),
                "BASE_SEPOLIA",
                84532,
                &factory,
            )?;
            add_chain(
                &mut chains,
                ChainKey::Custom("arbitrum_sepolia".into()),
                "ARBITRUM_SEPOLIA",
                421614,
                &factory,
            )?;
            add_chain(
                &mut chains,
                ChainKey::Custom("optimism_sepolia".into()),
                "OPTIMISM_SEPOLIA",
                11155420,
                &factory,
            )?;
            add_chain(
                &mut chains,
                ChainKey::Custom("polygon_amoy".into()),
                "POLYGON_AMOY",
                80002,
                &factory,
            )?;
        }
        let model_provider = parse_model_provider()?;
        // Ollama is the sole investigative provider. Do not silently switch to a
        // hosted model when the local investigator is unavailable.
        let default_model = "qwen2.5-coder:7b";
        let default_endpoint = "http://127.0.0.1:11434/api/chat";
        Ok(Self {
            bind: env::var("FLOWPAY_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            database_url: required("DATABASE_URL")?,
            environment: env::var("FLOWPAY_ENV").unwrap_or_else(|_| "local".into()),
            checkout_base_url: env::var("FLOWPAY_CHECKOUT_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3001".into())
                .trim_end_matches('/')
                .to_owned(),
            api_key_pepper: required("FLOWPAY_API_KEY_HASH_PEPPER")?,
            proxy_creation_code_hash: required("FLOWPAY_PROXY_CREATION_CODE_HASH")?,
            factory_runtime_code_hash: env::var("FLOWPAY_FACTORY_RUNTIME_CODE_HASH").ok(),
            operator_address: required("FLOWPAY_OPERATOR_ADDRESS")?,
            operator_private_key: env::var("FLOWPAY_OPERATOR_PRIVATE_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            faucet_address: env::var("FLOWPAY_FAUCET_ADDRESS").ok(),
            evidence_dir: env::var("FLOWPAY_EVIDENCE_DIR")
                .unwrap_or_else(|_| "./runtime/evidence".into()),
            webhook_encryption_key: parse_hex32("FLOWPAY_WEBHOOK_ENCRYPTION_KEY")?.to_vec(),
            chains,
            agent_mode: parse_agent_mode()?,
            model_provider,
            openai_api_key: env::var("OPENAI_API_KEY")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            openai_model: env::var("FLOWPAY_AGENT_MODEL").unwrap_or_else(|_| default_model.into()),
            openai_endpoint: env::var("FLOWPAY_MODEL_ENDPOINT")
                .or_else(|_| env::var("FLOWPAY_OPENAI_RESPONSES_URL"))
                .unwrap_or_else(|_| default_endpoint.into()),
            agent_max_steps: parse_usize("FLOWPAY_AGENT_MAX_STEPS", 12)?,
            agent_retry_budget: parse_usize("FLOWPAY_AGENT_RETRY_BUDGET", 3)?.clamp(1, 10),
            rabbitmq_url: env::var("RABBITMQ_URL")
                .unwrap_or_else(|_| "amqp://guest:guest@127.0.0.1:5672/%2f".into()),
            provider_webhook_secret: env::var("FLOWPAY_PROVIDER_WEBHOOK_SECRET")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            provider_webhook_path: env::var("FLOWPAY_PROVIDER_WEBHOOK_PATH")
                .unwrap_or_else(|_| "/v1/providers/alchemy/webhook".into()),
            alchemy_api_key: env::var("ALCHEMY_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            alchemy_networks: env::var("ALCHEMY_NETWORKS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect(),
        })
    }

    /// Returns a stable dev-mode merchant ID used when no API key is configured.
    #[must_use]
    pub fn default_dev_merchant(&self) -> flowpay_domain::MerchantId {
        use uuid::Uuid;
        // Deterministic UUID so it maps to the seeded dev merchant.
        flowpay_domain::MerchantId(
            Uuid::parse_str("11111111-1111-4111-8111-111111111111")
                .expect("hardcoded dev merchant UUID"),
        )
    }
}
fn add_chain(
    chains: &mut HashMap<ChainKey, ChainConfig>,
    chain: ChainKey,
    env_prefix: &str,
    default_chain_id: u64,
    default_factory: &str,
) -> anyhow::Result<()> {
    let rpc_key = format!("{env_prefix}_RPC_URL");
    let Some(rpc_url) = env::var(&rpc_key)
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(());
    };
    let chain_id_key = format!("{env_prefix}_CHAIN_ID");
    let factory_key = format!("{env_prefix}_FACTORY_ADDRESS");
    let configured_factory = env::var(&factory_key)
        .ok()
        .filter(|value| !value.trim().is_empty());
    if !env_prefix.eq("BASE") && !env_prefix.eq("BSC") && configured_factory.is_none() {
        return Err(anyhow!(
            "{factory_key} is required when {rpc_key} is configured"
        ));
    }
    let factory_address = configured_factory.unwrap_or_else(|| default_factory.to_owned());
    chains.insert(
        chain.clone(),
        ChainConfig {
            chain,
            rpc_url,
            numeric_chain_id: parse_u64(&chain_id_key, default_chain_id)?,
            factory_address,
        },
    );
    Ok(())
}

fn required(key: &str) -> anyhow::Result<String> {
    env::var(key).with_context(|| format!("missing {key}"))
}
fn parse_u64(key: &str, default: u64) -> anyhow::Result<u64> {
    match env::var(key) {
        Ok(v) => u64::from_str(&v).map_err(|e| anyhow!("invalid {key}: {e}")),
        Err(_) => Ok(default),
    }
}
fn parse_usize(key: &str, default: usize) -> anyhow::Result<usize> {
    match env::var(key) {
        Ok(v) => usize::from_str(&v).map_err(|e| anyhow!("invalid {key}: {e}")),
        Err(_) => Ok(default),
    }
}

fn parse_hex32(key: &str) -> anyhow::Result<[u8; 32]> {
    let value = required(key)?;
    let bytes = hex::decode(value.trim_start_matches("0x"))
        .with_context(|| format!("{key} must be hex"))?;
    bytes
        .try_into()
        .map_err(|_| anyhow!("{key} must be 32 bytes"))
}

fn parse_agent_mode() -> anyhow::Result<String> {
    let value = env::var("FLOWPAY_AGENT_MODE")
        .unwrap_or_else(|_| "model".into())
        .to_ascii_lowercase();
    match value.as_str() {
        "model" | "deterministic" | "baseline" => Ok(value),
        _ => Err(anyhow!(
            "FLOWPAY_AGENT_MODE must be model, deterministic, or baseline"
        )),
    }
}
fn parse_model_provider() -> anyhow::Result<String> {
    let value = env::var("FLOWPAY_MODEL_PROVIDER")
        .unwrap_or_else(|_| "ollama".into())
        .to_ascii_lowercase();
    match value.as_str() {
        "ollama" => Ok(value),
        _ => Err(anyhow!(
            "FlowPay uses Ollama as its only investigative provider; set FLOWPAY_MODEL_PROVIDER=ollama"
        )),
    }
}
