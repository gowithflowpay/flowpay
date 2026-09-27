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
    pub provider_webhook_secrets: Vec<String>,
    pub provider_webhook_path: String,
    pub provider_webhook_url: Option<String>,
    pub alchemy_api_key: Option<String>,
    pub alchemy_networks: Vec<String>,
    pub alchemy_notify_auth_token: Option<String>,
    pub alchemy_webhook_ids: HashMap<ChainKey, String>,
    pub alchemy_notify_endpoint: String,
    pub flutterwave_base_url: String,
    pub flutterwave_secret_key: Option<String>,
    pub flutterwave_secret_hash: Option<String>,
    pub flutterwave_customer_email: String,
    pub flutterwave_customer_first_name: String,
    pub flutterwave_customer_last_name: String,
    /// Flat platform fee, in minor NGN units, added to every bank-transfer
    /// total so the platform earns revenue on each collected payment.
    pub ngn_platform_fee_atomic: u64,
    /// Credential for the platform admin surface (revenue reporting).
    pub admin_key: Option<String>,
    /// Flutterwave's transaction fee, in basis points of the charged amount.
    pub flutterwave_fee_bps: u64,
    /// VAT charged on Flutterwave's fee, in basis points of that fee.
    pub flutterwave_fee_vat_bps: u64,
    pub formance_base_url: String,
    pub formance_ledger: String,
    pub formance_token: Option<String>,
    /// Outbound SMTP relay used for signup and login verification codes.
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub mail_from: String,
    pub mail_from_name: String,
    /// How long a session stays valid before the merchant must sign in again.
    pub auth_session_ttl_hours: i64,
    /// How long an emailed verification code stays usable.
    pub auth_code_ttl_minutes: i64,
    /// Public base URL of the merchant dashboard, used in any links we email.
    pub dashboard_base_url: String,
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
            provider_webhook_secrets: env::var("FLOWPAY_PROVIDER_WEBHOOK_SECRETS")
                .ok()
                .into_iter()
                .flat_map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .chain(
                    env::var("FLOWPAY_PROVIDER_WEBHOOK_SECRET")
                        .ok()
                        .into_iter()
                        .filter(|value| !value.trim().is_empty()),
                )
                .collect(),
            provider_webhook_path: env::var("FLOWPAY_PROVIDER_WEBHOOK_PATH")
                .unwrap_or_else(|_| "/v1/providers/alchemy/webhook".into()),
            provider_webhook_url: env::var("FLOWPAY_PROVIDER_WEBHOOK_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
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
            alchemy_notify_auth_token: env::var("ALCHEMY_NOTIFY_AUTH_TOKEN")
                .or_else(|_| env::var("ALCHEMY_API_KEY"))
                .ok()
                .filter(|value| !value.trim().is_empty()),
            alchemy_webhook_ids: parse_alchemy_webhook_ids(),
            alchemy_notify_endpoint: env::var("ALCHEMY_NOTIFY_ENDPOINT").unwrap_or_else(|_| {
                "https://dashboard.alchemy.com/api/update-webhook-addresses".into()
            }),
            flutterwave_base_url: env::var("FLW_BASE_URL")
                .unwrap_or_else(|_| "https://api.flutterwave.com/v3".into())
                .trim_end_matches('/')
                .to_owned(),
            flutterwave_secret_key: env::var("FLW_V3_SECRET_KEY")
                .or_else(|_| env::var("FLW_SECRET_KEY"))
                .ok()
                .filter(|value| !value.trim().is_empty()),
            flutterwave_secret_hash: env::var("FLW_SECRET_HASH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            flutterwave_customer_email: env::var("FLW_CUSTOMER_EMAIL")
                .unwrap_or_else(|_| "info@landaa.xyz".into()),
            flutterwave_customer_first_name: env::var("FLW_CUSTOMER_FIRST_NAME")
                .unwrap_or_else(|_| "FlowPay".into()),
            flutterwave_customer_last_name: env::var("FLW_CUSTOMER_LAST_NAME")
                .unwrap_or_else(|_| String::new()),
            ngn_platform_fee_atomic: env::var("FLOWPAY_NGN_PLATFORM_FEE")
                .ok()
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(5_000),
            admin_key: env::var("FLOWPAY_ADMIN_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            flutterwave_fee_bps: env::var("FLW_FEE_BPS")
                .ok()
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(200),
            flutterwave_fee_vat_bps: env::var("FLW_FEE_VAT_BPS")
                .ok()
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(750),
            formance_base_url: env::var("FORMANCE_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".into())
                .trim_end_matches('/')
                .to_owned(),
            formance_ledger: env::var("FORMANCE_LEDGER").unwrap_or_else(|_| "flowpay".into()),
            formance_token: env::var("FORMANCE_TOKEN")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            smtp_host: env::var("SMTP_HOST")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            smtp_port: env::var("SMTP_PORT")
                .ok()
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(587),
            smtp_username: env::var("SMTP_USERNAME")
                .or_else(|_| env::var("SMTP_USER"))
                .ok()
                .filter(|value| !value.trim().is_empty()),
            smtp_password: env::var("SMTP_PASSWORD")
                .or_else(|_| env::var("SMTP_PASS"))
                .ok()
                .filter(|value| !value.trim().is_empty()),
            mail_from: env::var("MAIL_FROM")
                .or_else(|_| env::var("SMTP_USERNAME"))
                .or_else(|_| env::var("SMTP_USER"))
                .unwrap_or_else(|_| "no-reply@flowpay.xyz".into()),
            mail_from_name: env::var("MAIL_FROM_NAME").unwrap_or_else(|_| "FlowPay".into()),
            auth_session_ttl_hours: env::var("FLOWPAY_AUTH_SESSION_TTL_HOURS")
                .ok()
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(24 * 14),
            auth_code_ttl_minutes: env::var("FLOWPAY_AUTH_CODE_TTL_MINUTES")
                .ok()
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(15),
            dashboard_base_url: env::var("FLOWPAY_DASHBOARD_BASE_URL")
                .unwrap_or_else(|_| "https://pixuno.xyz".into())
                .trim_end_matches('/')
                .to_owned(),
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

fn parse_alchemy_webhook_ids() -> HashMap<ChainKey, String> {
    [
        ("ALCHEMY_BSC_TESTNET_WEBHOOK_ID", "bsc_testnet"),
        ("ALCHEMY_ETHEREUM_SEPOLIA_WEBHOOK_ID", "ethereum_sepolia"),
        ("ALCHEMY_BASE_SEPOLIA_WEBHOOK_ID", "base_sepolia"),
        ("ALCHEMY_ARBITRUM_SEPOLIA_WEBHOOK_ID", "arbitrum_sepolia"),
        ("ALCHEMY_OPTIMISM_SEPOLIA_WEBHOOK_ID", "optimism_sepolia"),
        ("ALCHEMY_POLYGON_AMOY_WEBHOOK_ID", "polygon_amoy"),
    ]
    .into_iter()
    .filter_map(|(environment_key, chain)| {
        env::var(environment_key)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| (ChainKey::Custom(chain.into()), value))
    })
    .collect()
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
