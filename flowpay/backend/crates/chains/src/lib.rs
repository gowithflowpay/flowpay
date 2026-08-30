use async_trait::async_trait;
use flowpay_domain::{AddressRef, AtomicAmount, ChainKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainIdentity {
    pub key: ChainKey,
    pub numeric_chain_id: Option<u64>,
    pub genesis_hash: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainHealth {
    pub healthy: bool,
    pub latest_height: u64,
    pub provider_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainTransaction {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub block_number: Option<u64>,
    pub block_hash: Option<String>,
    pub success: Option<bool>,
    pub native_value: AtomicAmount,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub transaction_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub success: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub contract: String,
    pub symbol: String,
    pub decimals: u8,
    pub code_hash: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransferEvent {
    pub chain: ChainKey,
    pub tx_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub log_index: Option<u64>,
    pub from: String,
    pub to: String,
    pub token_contract: Option<String>,
    pub amount: AtomicAmount,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConfirmationStatus {
    pub observed_block: u64,
    pub canonical_block_hash: String,
    pub latest_block: u64,
    pub confirmations: u64,
    pub final_enough: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FactoryVerification {
    pub factory: String,
    pub has_code: bool,
    pub runtime_code_hash: Option<String>,
    pub expected_code_hash_matches: bool,
    pub deployer_authority_matches: bool,
    pub prediction_vector_matches: bool,
    pub recovery_capable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparedTransaction {
    pub transaction_class: String,
    pub chain: ChainKey,
    pub to: String,
    pub value: AtomicAmount,
    pub calldata_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SimulationResult {
    pub success: bool,
    pub gas_estimate: Option<AtomicAmount>,
    pub revert_reason: Option<String>,
    pub balance_deltas_verified: bool,
}

#[derive(Debug, Error)]
pub enum ChainError {
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("chain identity mismatch")]
    ChainIdentityMismatch,
    #[error("transaction not found")]
    TransactionNotFound,
    #[error("transaction is not canonical")]
    NonCanonical,
    #[error("unsupported operation: {0}")]
    Unsupported(String),
    #[error("invalid chain data: {0}")]
    InvalidData(String),
}

#[async_trait]
pub trait ChainAdapter: Send + Sync {
    fn identity(&self) -> &ChainIdentity;

    async fn health(&self) -> Result<ChainHealth, ChainError>;
    async fn transaction(&self, hash: &str) -> Result<ChainTransaction, ChainError>;
    async fn receipt(&self, hash: &str) -> Result<TransactionReceipt, ChainError>;
    async fn transaction_transfers(&self, hash: &str) -> Result<Vec<TransferEvent>, ChainError>;
    async fn confirmation_status(
        &self,
        block_number: u64,
        block_hash: &str,
        required_confirmations: u64,
    ) -> Result<ConfirmationStatus, ChainError>;
    async fn transfers_to(
        &self,
        address: &AddressRef,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<TransferEvent>, ChainError>;
    async fn token_metadata(&self, contract: &str) -> Result<TokenMetadata, ChainError>;
    async fn token_balance(
        &self,
        token_contract: &str,
        address: &str,
    ) -> Result<AtomicAmount, ChainError>;
    async fn native_balance(&self, address: &str) -> Result<AtomicAmount, ChainError>;
    async fn has_code(&self, address: &str) -> Result<bool, ChainError>;
    async fn simulate(&self, tx: &PreparedTransaction) -> Result<SimulationResult, ChainError>;
}

#[async_trait]
pub trait CounterfactualEvmAdapter: ChainAdapter {
    async fn compute_checkout_address(
        &self,
        factory: &str,
        salt_hex: &str,
    ) -> Result<String, ChainError>;

    async fn verify_factory(&self, factory: &str) -> Result<FactoryVerification, ChainError>;
}
